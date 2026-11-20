use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ncview_rs::{
    app::AppState,
    data::slice::{Bounds, Slice2D, Validity},
    events::input::command_from_event,
    render::{colors::Palette, raster::rgb_raster},
};
use ndarray::{Array2, arr2};

#[test]
fn vertical_flip_reverses_display_without_changing_source_values() {
    let slice = Slice2D::new(
        arr2(&[[1.0, 2.0], [3.0, 4.0]]),
        Array2::from_elem((2, 2), Validity::Finite),
        Bounds::new(0, 2, 0, 2).unwrap(),
    )
    .unwrap();
    let mut state = AppState::default();
    state.set_slice(slice);
    let original = rgb_raster(state.view.slice.as_ref().unwrap(), Palette::Viridis);
    let key = || Event::Key(KeyEvent::new(KeyCode::Char('F'), KeyModifiers::SHIFT));
    state.reduce(command_from_event(key()).expect("vertical flip shortcut"));
    let flipped = rgb_raster(state.view.slice.as_ref().unwrap(), Palette::Viridis);
    assert_eq!(original.get_pixel(0, 0), flipped.get_pixel(0, 1));
    assert_eq!(original.get_pixel(0, 1), flipped.get_pixel(0, 0));
    assert_eq!(state.view.slice.as_ref().unwrap().values[(0, 0)], 1.0);
    state.reduce(command_from_event(key()).expect("vertical flip shortcut"));
    assert_eq!(
        original,
        rgb_raster(state.view.slice.as_ref().unwrap(), Palette::Viridis)
    );
}
