use ncview_rs::{
    analysis::timeseries::point_across_time,
    app::{AppState, Command},
    data::slice::{Bounds, Slice2D, Validity},
};
use ndarray::Array2;

#[test]
fn cursor_series_preserves_gaps_and_axis_permutation_preserves_values() {
    let series =
        point_across_time(&[vec![Some(1.0), None], vec![Some(3.0), Some(4.0)]], 0, 1).unwrap();
    assert_eq!(series[0].value, None);
    assert_eq!(series[1].value, Some(4.0));

    let values = Array2::from_shape_vec((2, 3), vec![1., 2., 3., 4., 5., 6.]).unwrap();
    let mask = Array2::from_elem((2, 3), Validity::Finite);
    let slice = Slice2D::new(values, mask, Bounds::new(0, 2, 0, 3).unwrap()).unwrap();
    let swapped = slice.permuted_axes().unwrap();
    assert_eq!(swapped.values.shape(), &[3, 2]);
    assert_eq!(swapped.values[(2, 1)], 6.0);
}

#[test]
fn axis_assignment_rejects_duplicates_and_accepts_distinct_axes() {
    let mut state = AppState::default();
    state.reduce(Command::SetAxes {
        x: "lat".into(),
        y: "lat".into(),
    });
    assert!(state.view.x_axis.is_none());
    assert!(state.view.status.contains("distinct"));
    state.reduce(Command::SetAxes {
        x: "lon".into(),
        y: "lat".into(),
    });
    assert_eq!(state.view.x_axis.as_deref(), Some("lon"));
    assert_eq!(state.view.y_axis.as_deref(), Some("lat"));
}
