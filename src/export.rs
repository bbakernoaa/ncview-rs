//! File exporters for the current scientific view.

use std::{env, fmt::Write as FmtWrite, fs, io::Cursor, path::Path};

use base64_simd::STANDARD;
use image::{
    DynamicImage, ImageFormat, RgbImage, Rgba, RgbaImage,
    imageops::{self, FilterType},
};

use crate::{
    app::ScaleMode,
    render::colors::{Palette, colorbar},
    ui::colorbar::legend_ticks,
};

/// Write a self-contained SVG containing the map raster, metadata, and a
/// labeled colorbar. The raster is embedded as PNG so the export has no
/// dependency on the source NetCDF file.
#[allow(clippy::too_many_arguments)]
pub fn write_slice_svg(
    path: &Path,
    raster: &RgbImage,
    palette: &Palette,
    limits: (f64, f64),
    scale: ScaleMode,
    variable: &str,
    units: Option<&str>,
    long_name: Option<&str>,
    standard_name: Option<&str>,
    time_label: Option<&str>,
    depth_index: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut png = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(raster.clone()).write_to(&mut png, ImageFormat::Png)?;
    let encoded = STANDARD.encode_to_string(png.get_ref());

    // Use a 16:9 canvas with generous margins so the export can be dropped
    // directly into a presentation without an additional crop or resize.
    let height = 900.0;
    let map_x = 56.0;
    let map_y = 142.0;
    let map_width = 1240.0;
    let map_height = 700.0;
    let bar_x = 1340.0;
    let bar_y = 220.0;
    let bar_width = 42.0;
    let bar_height = 520.0;
    let (min, max) = limits;
    // Keep each metadata field intact on its own line. This avoids the
    // truncation that occurs when long COARDS/CF names are forced into a
    // single subtitle line, and makes the result easier to read on slides.
    let metadata_lines = [
        format!(
            "TIME: {}  |  Depth Index: {}",
            time_label.unwrap_or("TIME"),
            depth_index
        ),
        format!("Long_Name: {}", long_name.unwrap_or("—")),
        format!("Standard_Name: {}", standard_name.unwrap_or("—")),
        format!("units: {}", units.unwrap_or("—")),
    ];
    let max_line_chars = metadata_lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0);
    let width = 1600.0_f64.max(120.0 + max_line_chars as f64 * 7.0);

    // Transparent is the default so exports can be composited onto slides.
    // Set NCVIEW_EXPORT_BACKGROUND=white for an opaque presentation canvas.
    let transparent = transparent_export_background();
    let light_text = export_light_text();
    let text_primary = if light_text { "#f8fafc" } else { "#1f2937" };
    let text_secondary = if light_text { "#cbd5e1" } else { "#374151" };
    let text_muted = if light_text { "#94a3b8" } else { "#4b5563" };
    let border = if light_text { "#cbd5e1" } else { "#6b7280" };
    let font_family = env::var("NCVIEW_EXPORT_FONT")
        .unwrap_or_else(|_| "JetBrainsMono Nerd Font, Symbols Nerd Font, sans-serif".into());
    let font_family = escape_xml(&font_family);
    let mut svg = String::new();
    writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">"#
    )?;
    writeln!(
        svg,
        "<title>{}</title><desc>Scientific data slice exported by ncv</desc>",
        escape_xml(variable)
    )?;
    if !transparent {
        writeln!(
            svg,
            "<rect width=\"100%\" height=\"100%\" fill=\"#ffffff\"/>"
        )?;
    }
    writeln!(
        svg,
        "<text x=\"56\" y=\"52\" fill=\"{text_primary}\" font-family=\"{font_family}\" font-size=\"30\" font-weight=\"bold\">{}</text>",
        escape_xml(variable)
    )?;
    for (index, line) in metadata_lines.iter().enumerate() {
        let y = 78.0 + index as f64 * 16.0;
        let color = if index == 0 {
            text_secondary
        } else {
            text_muted
        };
        writeln!(
            svg,
            "<text x=\"56\" y=\"{y:.1}\" fill=\"{color}\" font-family=\"{font_family}\" font-size=\"13\">{}</text>",
            escape_xml(line)
        )?;
    }
    writeln!(
        svg,
        "<image x=\"{map_x}\" y=\"{map_y}\" width=\"{map_width}\" height=\"{map_height}\" preserveAspectRatio=\"none\" href=\"data:image/png;base64,{encoded}\"/>"
    )?;
    writeln!(
        svg,
        "<rect x=\"{map_x}\" y=\"{map_y}\" width=\"{map_width}\" height=\"{map_height}\" rx=\"3\" fill=\"none\" stroke=\"{border}\" stroke-width=\"2\"/>"
    )?;

    let colors = colorbar(palette, 64);
    let segment_height = bar_height / colors.len() as f64;
    for (index, rgb) in colors.into_iter().enumerate() {
        let y = bar_y + bar_height - (index as f64 + 1.0) * segment_height;
        writeln!(
            svg,
            "<rect x=\"{bar_x}\" y=\"{y:.2}\" width=\"{bar_width}\" height=\"{:.2}\" fill=\"#{:02x}{:02x}{:02x}\"/>",
            segment_height + 0.5,
            rgb[0],
            rgb[1],
            rgb[2]
        )?;
    }
    writeln!(
        svg,
        "<text x=\"{bar_x}\" y=\"180\" fill=\"{text_primary}\" font-family=\"{font_family}\" font-size=\"18\" font-weight=\"bold\">{}</text>",
        escape_xml(units.unwrap_or("value"))
    )?;
    writeln!(
        svg,
        "<rect x=\"{bar_x}\" y=\"{bar_y}\" width=\"{bar_width}\" height=\"{bar_height}\" fill=\"none\" stroke=\"{border}\"/>"
    )?;
    for (value, label) in legend_ticks(min, max, scale) {
        let fraction = fraction_for(value, min, max, scale);
        let y = bar_y + bar_height * (1.0 - fraction);
        writeln!(
            svg,
            "<line x1=\"{}\" y1=\"{y:.2}\" x2=\"{}\" y2=\"{y:.2}\" stroke=\"{text_secondary}\"/><text x=\"{}\" y=\"{:.2}\" dy=\"0.35em\" fill=\"{text_secondary}\" font-family=\"{font_family}\" font-size=\"15\">{}</text>",
            bar_x + bar_width,
            bar_x + bar_width + 8.0,
            bar_x + bar_width + 12.0,
            y,
            escape_xml(&label)
        )?;
    }
    svg.push_str("</svg>\n");
    fs::write(path, svg)?;
    Ok(())
}

/// Write a slide-compatible PNG containing the map, a colorbar, and tick marks.
/// The SVG companion remains the annotated/editable export; this raster format
/// is intended for direct insertion into Google Slides, PowerPoint, reports,
/// and notebooks that do not preserve SVG text/layout.
#[allow(clippy::too_many_arguments)]
pub fn write_slice_png(
    path: &Path,
    raster: &RgbImage,
    palette: &Palette,
    limits: (f64, f64),
    scale: ScaleMode,
    variable: &str,
    units: Option<&str>,
    long_name: Option<&str>,
    standard_name: Option<&str>,
    time_label: Option<&str>,
    depth_index: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    const WIDTH: u32 = 1600;
    const HEIGHT: u32 = 900;
    const MAP_X: u32 = 56;
    const MAP_Y: u32 = 142;
    const MAP_WIDTH: u32 = 1240;
    const MAP_HEIGHT: u32 = 700;
    const BAR_X: u32 = 1340;
    const BAR_Y: u32 = 220;
    const BAR_WIDTH: u32 = 42;
    const BAR_HEIGHT: u32 = 520;

    let transparent = transparent_export_background();
    let background = if transparent {
        Rgba([0, 0, 0, 0])
    } else {
        Rgba([255, 255, 255, 255])
    };
    let foreground = if export_light_text() {
        Rgba([203, 213, 225, 255])
    } else {
        Rgba([55, 65, 81, 255])
    };
    let mut canvas = RgbaImage::from_pixel(WIDTH, HEIGHT, background);
    let map = imageops::resize(raster, MAP_WIDTH, MAP_HEIGHT, FilterType::Nearest);
    let map = DynamicImage::ImageRgb8(map).to_rgba8();
    imageops::overlay(&mut canvas, &map, i64::from(MAP_X), i64::from(MAP_Y));

    let metadata_lines = [
        format!(
            "TIME: {}  |  DEPTH INDEX: {}",
            time_label.unwrap_or("TIME"),
            depth_index
        ),
        format!("LONG_NAME: {}", long_name.unwrap_or("—")),
        format!("STANDARD_NAME: {}", standard_name.unwrap_or("—")),
        format!("UNITS: {}", units.unwrap_or("—")),
    ];
    draw_text(&mut canvas, 56, 20, variable, foreground, 4);
    for (index, line) in metadata_lines.iter().enumerate() {
        draw_text(&mut canvas, 56, 64 + index as u32 * 16, line, foreground, 2);
    }

    // Frame the map and colorbar with a high-contrast, presentation-safe line.
    for x in MAP_X..MAP_X + MAP_WIDTH {
        for y in MAP_Y..MAP_Y + 2 {
            canvas.put_pixel(x, y, foreground);
            canvas.put_pixel(x, MAP_Y + MAP_HEIGHT - 1 - (y - MAP_Y), foreground);
        }
    }
    for y in MAP_Y..MAP_Y + MAP_HEIGHT {
        for x in MAP_X..MAP_X + 2 {
            canvas.put_pixel(x, y, foreground);
            canvas.put_pixel(MAP_X + MAP_WIDTH - 1 - (x - MAP_X), y, foreground);
        }
    }

    let colors = colorbar(palette, 64);
    for y in 0..BAR_HEIGHT {
        let index = ((BAR_HEIGHT - 1 - y) as usize * colors.len() / BAR_HEIGHT as usize)
            .min(colors.len() - 1);
        let rgb = colors[index];
        let color = Rgba([rgb[0], rgb[1], rgb[2], 255]);
        for x in BAR_X..BAR_X + BAR_WIDTH {
            canvas.put_pixel(x, BAR_Y + y, color);
        }
    }
    for x in BAR_X..BAR_X + BAR_WIDTH {
        for y in BAR_Y..BAR_Y + 2 {
            canvas.put_pixel(x, y, foreground);
            canvas.put_pixel(x, BAR_Y + BAR_HEIGHT - 1 - (y - BAR_Y), foreground);
        }
    }
    for y in BAR_Y..BAR_Y + BAR_HEIGHT {
        for x in BAR_X..BAR_X + 2 {
            canvas.put_pixel(x, y, foreground);
            canvas.put_pixel(BAR_X + BAR_WIDTH - 1 - (x - BAR_X), y, foreground);
        }
    }
    let (min, max) = limits;
    draw_text(
        &mut canvas,
        BAR_X,
        184,
        units.unwrap_or("VALUE"),
        foreground,
        3,
    );
    for (value, label) in legend_ticks(min, max, scale) {
        let fraction = fraction_for(value, min, max, scale);
        let y = BAR_Y + ((BAR_HEIGHT - 1) as f64 * (1.0 - fraction)).round() as u32;
        for x in BAR_X + BAR_WIDTH..(BAR_X + BAR_WIDTH + 12).min(WIDTH) {
            for offset in 0..2 {
                if y + offset < HEIGHT {
                    canvas.put_pixel(x, y + offset, foreground);
                }
            }
        }
        draw_text(
            &mut canvas,
            BAR_X + BAR_WIDTH + 18,
            y.saturating_sub(7),
            &label,
            foreground,
            2,
        );
    }
    canvas.save_with_format(path, ImageFormat::Png)?;
    Ok(())
}

/// Write machine-readable metadata alongside the raster export. This keeps
/// variable names, CF/COARDS attributes, limits, and slice coordinates intact
/// even in workflows that only ingest PNG files.
#[allow(clippy::too_many_arguments)]
pub fn write_slice_metadata_json(
    path: &Path,
    limits: (f64, f64),
    scale: ScaleMode,
    variable: &str,
    units: Option<&str>,
    long_name: Option<&str>,
    standard_name: Option<&str>,
    time_label: Option<&str>,
    depth_index: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let optional = |value: Option<&str>| {
        value.map_or_else(
            || "null".to_string(),
            |value| format!("\"{}\"", escape_json(value)),
        )
    };
    let scale = match scale {
        ScaleMode::Linear => "linear",
        ScaleMode::Log => "log",
    };
    let content = format!(
        "{{\n  \"variable\": \"{}\",\n  \"units\": {},\n  \"long_name\": {},\n  \"standard_name\": {},\n  \"time\": {},\n  \"depth_index\": {},\n  \"scale\": \"{}\",\n  \"limits\": [{:.17e}, {:.17e}]\n}}\n",
        escape_json(variable),
        optional(units),
        optional(long_name),
        optional(standard_name),
        optional(time_label),
        depth_index,
        scale,
        limits.0,
        limits.1,
    );
    fs::write(path, content)?;
    Ok(())
}

fn transparent_export_background() -> bool {
    !env::var("NCVIEW_EXPORT_BACKGROUND")
        .map(|value| value.eq_ignore_ascii_case("white"))
        .unwrap_or(false)
}

fn export_light_text() -> bool {
    env::var("NCVIEW_EXPORT_TEXT")
        .map(|value| value.eq_ignore_ascii_case("light"))
        .unwrap_or(false)
}

/// Draw presentation text without a system-font dependency. The compact 5x7
/// glyphs are deliberately embedded so PNG exports render identically on macOS,
/// Linux, Windows, and headless HPC login nodes.
fn draw_text(image: &mut RgbaImage, x: u32, y: u32, text: &str, color: Rgba<u8>, scale: u32) {
    let mut cursor_x = x;
    for character in text.chars() {
        if cursor_x >= image.width() {
            break;
        }
        let glyph = glyph_for(character);
        for (row, bits) in glyph.iter().enumerate() {
            for column in 0..5u32 {
                if bits & (1 << (4 - column)) == 0 {
                    continue;
                }
                for dy in 0..scale {
                    for dx in 0..scale {
                        let px = cursor_x + column * scale + dx;
                        let py = y + row as u32 * scale + dy;
                        if px < image.width() && py < image.height() {
                            image.put_pixel(px, py, color);
                        }
                    }
                }
            }
        }
        cursor_x = cursor_x.saturating_add(6 * scale);
    }
}

fn glyph_for(character: char) -> [u8; 7] {
    let character = match character {
        'µ' | 'μ' => 'u',
        '−' | '–' | '—' => '-',
        '²' => '2',
        '³' => '3',
        '·' | '×' => '*',
        '°' => 'o',
        character => character,
    };
    match character.to_ascii_uppercase() {
        'A' => [
            0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'B' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10001, 0b10001, 0b11110,
        ],
        'C' => [
            0b01111, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b01111,
        ],
        'D' => [
            0b11110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b11110,
        ],
        'E' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111,
        ],
        'F' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'G' => [
            0b01111, 0b10000, 0b10000, 0b10111, 0b10001, 0b10001, 0b01111,
        ],
        'H' => [
            0b10001, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
        ],
        'I' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b11111,
        ],
        'J' => [
            0b00111, 0b00010, 0b00010, 0b00010, 0b10010, 0b10010, 0b01100,
        ],
        'K' => [
            0b10001, 0b10010, 0b10100, 0b11000, 0b10100, 0b10010, 0b10001,
        ],
        'L' => [
            0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b10000, 0b11111,
        ],
        'M' => [
            0b10001, 0b11011, 0b10101, 0b10101, 0b10001, 0b10001, 0b10001,
        ],
        'N' => [
            0b10001, 0b11001, 0b10101, 0b10011, 0b10001, 0b10001, 0b10001,
        ],
        'O' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'P' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10000, 0b10000, 0b10000,
        ],
        'Q' => [
            0b01110, 0b10001, 0b10001, 0b10001, 0b10101, 0b10010, 0b01101,
        ],
        'R' => [
            0b11110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001,
        ],
        'S' => [
            0b01111, 0b10000, 0b10000, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        'T' => [
            0b11111, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'U' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01110,
        ],
        'V' => [
            0b10001, 0b10001, 0b10001, 0b10001, 0b10001, 0b01010, 0b00100,
        ],
        'W' => [
            0b10001, 0b10001, 0b10001, 0b10101, 0b10101, 0b10101, 0b01010,
        ],
        'X' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b01010, 0b10001, 0b10001,
        ],
        'Y' => [
            0b10001, 0b10001, 0b01010, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        'Z' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0b11111,
        ],
        '0' => [
            0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110,
        ],
        '1' => [
            0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110,
        ],
        '2' => [
            0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111,
        ],
        '3' => [
            0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110,
        ],
        '4' => [
            0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010,
        ],
        '5' => [
            0b11111, 0b10000, 0b10000, 0b11110, 0b00001, 0b00001, 0b11110,
        ],
        '6' => [
            0b01110, 0b10000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110,
        ],
        '7' => [
            0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000,
        ],
        '8' => [
            0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110,
        ],
        '9' => [
            0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00001, 0b01110,
        ],
        '-' => [0, 0, 0, 0b11111, 0, 0, 0],
        '_' => [0, 0, 0, 0, 0, 0, 0b11111],
        ':' => [0, 0b00100, 0, 0, 0b00100, 0, 0],
        '.' => [0, 0, 0, 0, 0, 0b00110, 0b00110],
        ',' => [0, 0, 0, 0, 0, 0b00110, 0b00100],
        '/' => [0b00001, 0b00010, 0b00100, 0b01000, 0b10000, 0, 0],
        '+' => [0, 0b00100, 0b00100, 0b11111, 0b00100, 0b00100, 0],
        '=' => [0, 0, 0b11111, 0, 0b11111, 0, 0],
        '|' => [
            0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100, 0b00100,
        ],
        '(' => [
            0b00010, 0b00100, 0b01000, 0b01000, 0b01000, 0b00100, 0b00010,
        ],
        ')' => [
            0b01000, 0b00100, 0b00010, 0b00010, 0b00010, 0b00100, 0b01000,
        ],
        '%' => [
            0b11001, 0b11010, 0b00010, 0b00100, 0b01000, 0b01011, 0b10011,
        ],
        '*' => [0, 0b10101, 0b01110, 0b11111, 0b01110, 0b10101, 0],
        ' ' => [0; 7],
        _ => [0b01110, 0b10001, 0b00010, 0b00100, 0b00100, 0, 0b00100],
    }
}

fn escape_json(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn fraction_for(value: f64, min: f64, max: f64, scale: ScaleMode) -> f64 {
    match scale {
        ScaleMode::Linear => ((value - min) / (max - min)).clamp(0.0, 1.0),
        ScaleMode::Log if min > 0.0 && max > 0.0 && value > 0.0 => {
            ((value.log10() - min.log10()) / (max.log10() - min.log10())).clamp(0.0, 1.0)
        }
        ScaleMode::Log => 0.0,
    }
}

fn escape_xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use image::{GenericImageView, Rgb, RgbImage};

    use super::{write_slice_metadata_json, write_slice_png, write_slice_svg};
    use crate::{app::ScaleMode, render::colors::Palette};

    #[test]
    fn writes_embedded_image_and_labeled_colorbar() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("slice.svg");
        let png_path = directory.path().join("slice.png");
        let metadata_path = directory.path().join("slice.json");
        let mut raster = RgbImage::new(2, 2);
        raster.put_pixel(0, 0, Rgb([255, 0, 0]));
        write_slice_svg(
            &path,
            &raster,
            &Palette::Viridis,
            (0.0, 1.0),
            ScaleMode::Linear,
            "temperature",
            Some("K"),
            Some("surface temperature"),
            None,
            Some("t=0"),
            0,
        )
        .unwrap();
        let output = std::fs::read_to_string(path).unwrap();
        assert!(output.contains("data:image/png;base64,"));
        assert!(output.contains("surface temperature"));
        assert!(output.contains("TIME") || output.contains("t=0"));
        assert!(output.contains("Long_Name:"));
        assert!(output.contains("Standard_Name"));
        assert!(output.contains("units: K"));
        assert!(output.contains("x=\"1340\""));

        write_slice_png(
            &png_path,
            &raster,
            &Palette::Viridis,
            (0.0, 1.0),
            ScaleMode::Linear,
            "temperature",
            Some("K"),
            Some("surface temperature"),
            None,
            Some("t=0"),
            0,
        )
        .unwrap();
        let png = image::open(png_path).unwrap();
        assert_eq!(png.dimensions(), (1600, 900));
        assert_ne!(png.to_rgba8().get_pixel(56, 20).0, [0, 0, 0, 0]);

        write_slice_metadata_json(
            &metadata_path,
            (0.0, 1.0),
            ScaleMode::Linear,
            "temperature",
            Some("K"),
            Some("surface temperature"),
            None,
            Some("t=0"),
            0,
        )
        .unwrap();
        let metadata = std::fs::read_to_string(metadata_path).unwrap();
        assert!(metadata.contains("\"long_name\": \"surface temperature\""));
        assert!(metadata.contains("\"units\": \"K\""));
    }
}
