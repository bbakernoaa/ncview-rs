use ncview_rs::{
    app::{AppState, Command, Overlay},
    data::slice::{Bounds, Slice2D, Validity},
    events::mouse::DragState,
    render::colors::{
        Palette, ScaleMode, color_for_with_limits, color_for_with_limits_and_filter_and_scale,
    },
};
use ndarray::Array2;
use ratatui::layout::Rect;

#[test]
fn palette_limits_zoom_and_reset_are_reversible() {
    let mut state = AppState::default();
    let values = Array2::from_elem((2, 2), 1.0);
    let mask = Array2::from_elem((2, 2), Validity::Finite);
    state.view.slice = Some(Slice2D::new(values, mask, Bounds::new(0, 2, 0, 2).unwrap()).unwrap());
    state.reduce(Command::CyclePalette);
    assert_eq!(state.view.palette.name(), "Plasma");
    state.reduce(Command::AutomaticLimits);
    assert_eq!(state.view.limits, Some((1.0, 1.0)));
    state.reduce(Command::ManualLimits { min: 2.0, max: 1.0 });
    assert_eq!(state.view.status, "limits require finite min < max");
    let bounds = Bounds::new(0, 1, 0, 1).unwrap();
    state.reduce(Command::Zoom(bounds));
    assert_eq!(state.view.zoom_bounds, Some(bounds));
    state.reduce(Command::ResetZoom);
    assert_eq!(state.view.zoom_bounds, None);
    state.reduce(Command::OpenLimits);
    assert_eq!(state.view.overlay, Some(Overlay::Limits));
}

#[test]
fn reverse_drag_maps_to_nonempty_source_bounds() {
    let drag = DragState {
        start: (18, 12),
        current: (8, 4),
        zoom: false,
    };
    let bounds = drag.bounds(Rect::new(4, 2, 20, 12), 100, 200).unwrap();
    assert!(bounds.row_start < bounds.row_end);
    assert!(bounds.col_start < bounds.col_end);
}

#[test]
fn manual_limits_change_the_rendered_color_mapping() {
    let values = Array2::from_shape_vec((1, 2), vec![0.0, 100.0]).unwrap();
    let mask = Array2::from_elem((1, 2), Validity::Finite);
    let slice = Slice2D::new(values, mask, Bounds::new(0, 1, 0, 2).unwrap()).unwrap();
    let auto = color_for_with_limits(&slice, 0, 1, Palette::Viridis, None);
    let manual = color_for_with_limits(&slice, 0, 1, Palette::Viridis, Some((0.0, 200.0)));
    assert_ne!(auto, manual);
}

#[test]
fn axis_selector_applies_reversed_axes_to_the_current_slice() {
    let mut state = AppState::default();
    let values = Array2::from_shape_vec((2, 3), vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).unwrap();
    let mask = Array2::from_elem((2, 3), Validity::Finite);
    state.view.slice = Some(Slice2D::new(values, mask, Bounds::new(0, 2, 0, 3).unwrap()).unwrap());
    state.reduce(Command::SetAxes {
        x: "lat".into(),
        y: "lon".into(),
    });
    let slice = state.view.slice.as_ref().unwrap();
    assert_eq!(slice.values.shape(), &[3, 2]);
    assert_eq!(slice.values[(0, 1)], 4.0);
    assert_eq!(state.view.x_axis.as_deref(), Some("lat"));
}

#[test]
fn data_filter_masks_values_outside_the_requested_range() {
    let mut state = AppState::default();
    state.reduce(Command::OpenFilter);
    state.reduce(Command::InputChar('2'));
    state.reduce(Command::NextLimitField);
    state.reduce(Command::InputChar('8'));
    state.reduce(Command::ActivatePoint);
    assert_eq!(state.view.filter_range, Some((2.0, 8.0)));

    let values = Array2::from_shape_vec((1, 2), vec![1.0, 9.0]).unwrap();
    let mask = Array2::from_elem((1, 2), Validity::Finite);
    let slice = Slice2D::new(values, mask, Bounds::new(0, 1, 0, 2).unwrap()).unwrap();
    let filtered = ncview_rs::render::colors::color_for_with_limits_and_filter(
        &slice,
        0,
        0,
        ncview_rs::render::colors::Palette::Viridis,
        None,
        state.view.filter_range,
    );
    assert_eq!(filtered, [30, 30, 46]);
}

#[test]
fn logarithmic_scale_changes_color_normalization_and_rejects_nonpositive_values() {
    let values = Array2::from_shape_vec((1, 2), vec![10.0, -1.0]).unwrap();
    let mask = Array2::from_elem((1, 2), Validity::Finite);
    let slice = Slice2D::new(values, mask, Bounds::new(0, 1, 0, 2).unwrap()).unwrap();
    let linear = color_for_with_limits(&slice, 0, 0, Palette::Viridis, Some((1.0, 100.0)));
    let logarithmic = color_for_with_limits_and_filter_and_scale(
        &slice,
        0,
        0,
        Palette::Viridis,
        Some((1.0, 100.0)),
        None,
        ScaleMode::Log,
    );
    assert_ne!(linear, logarithmic);
    let invalid = color_for_with_limits_and_filter_and_scale(
        &slice,
        0,
        1,
        Palette::Viridis,
        Some((1.0, 100.0)),
        None,
        ScaleMode::Log,
    );
    assert_eq!(invalid, [80, 80, 80]);
}
