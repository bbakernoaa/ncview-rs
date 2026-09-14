use std::{
    env,
    hash::{Hash, Hasher},
    sync::mpsc::{self, Receiver},
};

use image::DynamicImage;
use image::imageops::FilterType;
use ratatui::{
    Frame,
    layout::{Rect, Size},
};
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::StatefulProtocol;
use ratatui_image::{FontSize, Resize, ResizeEncodeRender, StatefulImage};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageProtocol {
    Kitty,
    Sixel,
    Iterm2,
    Halfblocks,
}

pub struct ProtocolState {
    pub picker: Picker,
    pub protocol: ImageProtocol,
}

/// Stateful image renderer used when the terminal supports Kitty or Sixel.
/// The dashboard keeps the cell rasterizer as its portable fallback.
pub struct GraphicsRenderer {
    state: ProtocolState,
    image: Option<StatefulProtocol>,
    image_hash: Option<u64>,
    pending: Option<PendingImage>,
    image_bytes: usize,
    resize_filter: FilterType,
    scientific_mode: bool,
    disabled: bool,
}

struct PendingImage {
    hash: u64,
    bytes: usize,
    receiver: Receiver<Result<StatefulProtocol, String>>,
}

impl GraphicsRenderer {
    pub fn probe() -> Self {
        let scientific_mode = scientific_rendering_enabled();
        Self {
            state: ProtocolState::probe(),
            image: None,
            image_hash: None,
            pending: None,
            image_bytes: 0,
            resize_filter: if scientific_mode {
                FilterType::Nearest
            } else {
                image_filter()
            },
            scientific_mode,
            disabled: false,
        }
    }

    /// Create an independent image slot that shares the terminal capability
    /// query with the primary renderer. The dashboard uses one slot for the
    /// map and one for popup charts so resizing or re-encoding one image does
    /// not invalidate the other.
    pub fn secondary(&self) -> Self {
        Self {
            state: ProtocolState {
                picker: self.state.picker.clone(),
                protocol: self.state.protocol,
            },
            image: None,
            image_hash: None,
            pending: None,
            image_bytes: 0,
            resize_filter: self.resize_filter,
            scientific_mode: self.scientific_mode,
            disabled: self.disabled,
        }
    }

    pub fn supports_graphics(&self) -> bool {
        !self.disabled
            && matches!(
                self.state.protocol,
                ImageProtocol::Kitty | ImageProtocol::Sixel | ImageProtocol::Iterm2
            )
    }

    pub fn protocol(&self) -> ImageProtocol {
        self.state.protocol
    }

    pub fn filter_label(&self) -> &'static str {
        if self.scientific_mode {
            "nearest (locked)"
        } else {
            filter_label(self.resize_filter)
        }
    }

    /// Return the portion of a terminal area occupied by the proportional
    /// graphics image. Kitty, Sixel, and iTerm2 may letterbox an image rather
    /// than filling every cell in the panel; mouse mapping must use this same
    /// rectangle or point markers appear displaced from the click.
    pub fn drawable_area(&self, area: Rect) -> Rect {
        if !self.supports_graphics() || area.width == 0 || area.height == 0 {
            return area;
        }
        let Some(image) = self.image.as_ref() else {
            return area;
        };
        let size = image.size_for(
            Resize::Scale(Some(self.resize_filter)),
            Size::new(area.width, area.height),
        );
        Rect::new(
            area.x,
            area.y,
            size.width.min(area.width),
            size.height.min(area.height),
        )
    }

    pub fn cycle_filter(&mut self) -> bool {
        if self.scientific_mode {
            return false;
        }
        self.resize_filter = match self.resize_filter {
            FilterType::Nearest => FilterType::CatmullRom,
            FilterType::CatmullRom => FilterType::Lanczos3,
            FilterType::Lanczos3 => FilterType::Triangle,
            FilterType::Triangle => FilterType::Gaussian,
            FilterType::Gaussian => FilterType::Nearest,
        };
        // The encoded protocol image includes the resized pixels, so force a
        // fresh encoding when the interpolation mode changes.
        self.image = None;
        self.image_hash = None;
        self.pending = None;
        self.image_bytes = 0;
        true
    }

    /// Human-readable renderer mode for the dashboard header. This makes it
    /// obvious when a terminal is using a graphics protocol versus the cell fallback.
    pub fn mode_label(&self) -> &'static str {
        if self.disabled {
            return "cells (graphics unavailable)";
        }
        match self.state.protocol {
            ImageProtocol::Kitty => "Kitty truecolor",
            ImageProtocol::Sixel => "Sixel truecolor",
            ImageProtocol::Iterm2 => "iTerm2 truecolor",
            ImageProtocol::Halfblocks => "cell fallback",
        }
    }

    pub fn render(&mut self, frame: &mut Frame, area: Rect, image: DynamicImage) -> bool {
        if !self.supports_graphics() || area.width == 0 || area.height == 0 {
            return false;
        }
        let hash = image_hash(&image);
        self.finish_pending(hash);
        if !self.supports_graphics() {
            return false;
        }
        // Keep at most one encoder in flight. If playback advances while that
        // encoder is busy, the completed frame is discarded and the current
        // slice is queued on the next draw.
        if self.image_hash != Some(hash) && self.pending.is_none() {
            self.start_encoding(hash, image, area);
        }
        let Some(protocol) = self.image.as_mut() else {
            // Keep the map stable until the first protocol image has finished
            // encoding. Falling through to cell rendering here would produce
            // a one-frame flash when the protocol image arrives.
            return true;
        };
        frame.render_stateful_widget(
            // Nearest-neighbor is the default for scientific rasters: it
            // preserves cell boundaries and avoids inventing intermediate
            // colours when a zoomed slice is enlarged. Users who prefer a
            // photographic-looking interpolation can set
            // NCVIEW_IMAGE_FILTER=lanczos3 or catmull-rom.
            StatefulImage::new().resize(Resize::Scale(Some(self.resize_filter))),
            area,
            protocol,
        );
        if protocol
            .last_encoding_result()
            .is_some_and(|result| result.is_err())
        {
            self.image = None;
            self.image_hash = None;
            self.disabled = true;
            return false;
        }
        true
    }

    /// Whether a replacement image is being prepared. The event loop uses
    /// this to keep rendering while the old image remains on screen.
    pub fn has_pending_image(&self) -> bool {
        self.pending.is_some()
    }

    /// Bytes retained by the current encoded image and the replacement being
    /// prepared. The input raster is viewport-bounded, so this is the useful
    /// accounting boundary for rendered transport buffers.
    pub fn working_set_bytes(&self) -> usize {
        self.image_bytes
            .saturating_add(self.pending.as_ref().map_or(0, |pending| pending.bytes))
    }

    fn start_encoding(&mut self, hash: u64, image: DynamicImage, area: Rect) {
        let picker = self.state.picker.clone();
        let resize = Resize::Scale(Some(self.resize_filter));
        let size = Size::new(area.width, area.height);
        let bytes = image.as_bytes().len();
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut protocol = picker.new_resize_protocol(image);
            protocol.resize_encode(&resize, size);
            let result = match protocol.last_encoding_result() {
                Some(Ok(())) => Ok(protocol),
                Some(Err(_)) => Err("image protocol encoding failed".to_string()),
                None => Err("image protocol did not encode".to_string()),
            };
            let _ = sender.send(result);
        });
        self.pending = Some(PendingImage {
            hash,
            bytes,
            receiver,
        });
    }

    fn finish_pending(&mut self, current_hash: u64) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        match pending.receiver.try_recv() {
            Ok(Ok(protocol)) if pending.hash == current_hash => {
                self.image_bytes = pending.bytes;
                self.image = Some(protocol);
                self.image_hash = Some(current_hash);
            }
            Ok(Err(_)) if pending.hash == current_hash => {
                self.image_bytes = 0;
                self.disabled = true;
            }
            Ok(_) => {
                // A newer time slice is already being rendered. Discard this
                // completed frame; render() will queue the current one below.
            }
            Err(mpsc::TryRecvError::Empty) => {
                self.pending = Some(pending);
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                if pending.hash == current_hash {
                    self.disabled = true;
                }
            }
        }
    }
}

fn image_filter() -> FilterType {
    match env::var("NCVIEW_IMAGE_FILTER")
        .unwrap_or_else(|_| "nearest".into())
        .to_ascii_lowercase()
        .as_str()
    {
        "lanczos" | "lanczos3" => FilterType::Lanczos3,
        "catmull" | "catmull-rom" | "catmullrom" => FilterType::CatmullRom,
        "triangle" | "linear" => FilterType::Triangle,
        "gaussian" => FilterType::Gaussian,
        _ => FilterType::Nearest,
    }
}

fn scientific_rendering_enabled() -> bool {
    !matches!(
        env::var("NCVIEW_SCIENTIFIC_RENDERING")
            .unwrap_or_else(|_| "1".into())
            .to_ascii_lowercase()
            .as_str(),
        "0" | "false" | "no" | "off"
    )
}

fn filter_label(filter: FilterType) -> &'static str {
    match filter {
        FilterType::Nearest => "nearest",
        FilterType::CatmullRom => "catmull-rom",
        FilterType::Lanczos3 => "lanczos3",
        FilterType::Triangle => "triangle",
        FilterType::Gaussian => "gaussian",
    }
}

fn image_hash(image: &DynamicImage) -> u64 {
    let rgba = image.to_rgba8();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    image.width().hash(&mut hasher);
    image.height().hash(&mut hasher);
    rgba.as_raw().hash(&mut hasher);
    hasher.finish()
}

impl ProtocolState {
    pub fn fallback() -> Self {
        Self {
            picker: Picker::halfblocks(),
            protocol: ImageProtocol::Halfblocks,
        }
    }
    pub fn probe() -> Self {
        // Terminal capability queries can block for seconds when stdout is
        // redirected, SSH is involved, or a multiplexer swallows the reply.
        // Startup must never depend on that round trip, so use the
        // deterministic fallback and rely on explicit environment hints.
        let mut picker = picker_with_terminal_cell_size();
        // `from_query_stdio` can time out when iTerm2 is behind SSH or a
        // multiplexer. Preserve the terminal's environment hint in that
        // error path instead of silently falling back to half-block cells.
        if picker.protocol_type() == ProtocolType::Halfblocks && iterm2_hint() {
            picker.set_protocol_type(ProtocolType::Iterm2);
        }
        // Useful for terminals whose capability query is blocked by a
        // multiplexer. The renderer still self-disables if the forced
        // protocol cannot encode, so this is safe to experiment with.
        if let Ok(forced) = env::var("NCVIEW_IMAGE_PROTOCOL") {
            let protocol_type = match forced.to_ascii_lowercase().as_str() {
                "kitty" => Some(ProtocolType::Kitty),
                "sixel" => Some(ProtocolType::Sixel),
                "iterm2" | "iterm" => Some(ProtocolType::Iterm2),
                "cells" | "halfblocks" => Some(ProtocolType::Halfblocks),
                _ => None,
            };
            if let Some(protocol_type) = protocol_type {
                picker.set_protocol_type(protocol_type);
            }
        }
        let protocol = match picker.protocol_type() {
            ProtocolType::Kitty => ImageProtocol::Kitty,
            ProtocolType::Sixel => ImageProtocol::Sixel,
            ProtocolType::Iterm2 => ImageProtocol::Iterm2,
            _ => ImageProtocol::Halfblocks,
        };
        Self { picker, protocol }
    }
}

fn picker_with_terminal_cell_size() -> Picker {
    let font_size = crossterm::terminal::window_size().ok().and_then(|size| {
        let width = size.width.checked_div(size.columns)?;
        let height = size.height.checked_div(size.rows)?;
        (width > 0 && height > 0).then_some(FontSize::new(width, height))
    });
    match font_size {
        // This constructor is deprecated in favor of an active terminal
        // query. The query can hang at startup; window_size provides the same
        // geometry without terminal I/O.
        #[allow(deprecated)]
        Some(font_size) => Picker::from_fontsize(font_size),
        None => Picker::halfblocks(),
    }
}

fn iterm2_hint() -> bool {
    env::var("ITERM_SESSION_ID").is_ok_and(|value| !value.is_empty())
        || env::var("TERM_PROGRAM").is_ok_and(|value| value.to_ascii_lowercase().contains("iterm"))
        || env::var("LC_TERMINAL").is_ok_and(|value| value.to_ascii_lowercase().contains("iterm"))
}
