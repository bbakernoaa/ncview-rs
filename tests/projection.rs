use ncview_rs::{
    analysis::projection::{ProjectionIndex, normalize_longitude},
    data::coordinates::{GridClass, classify},
    render::raster::projected_lookup,
};

#[test]
fn coordinates_classify_curvilinear_and_reject_shape_mismatch() {
    let lat = [0.0, 0.2, 1.0, 1.2];
    let lon = [0.0, 1.0, 0.1, 1.1];
    assert_eq!(
        classify(&lat, &lon, 2, 2).unwrap().class,
        GridClass::Curvilinear
    );
    assert!(classify(&lat, &lon[..3], 2, 2).is_none());
}

#[test]
fn projection_normalizes_seam_and_breaks_ties_by_lowest_source_index() {
    assert_eq!(normalize_longitude(180.0), -180.0);
    assert_eq!(normalize_longitude(-540.0), -180.0);
    let index = ProjectionIndex::build(&[0.0, 0.0], &[-1.0, 1.0], 2);
    let nearest = index.nearest(0.0, 0.0).unwrap();
    assert_eq!((nearest.row, nearest.col), (0, 0));
}

#[test]
fn projected_lookup_keeps_source_traceability() {
    let index = ProjectionIndex::build(&[0.0, 0.0, 10.0, 10.0], &[-10.0, 10.0, -10.0, 10.0], 2);
    let lookup = projected_lookup(2, 2, &index);
    assert_eq!(lookup.len(), 4);
    assert!(lookup.iter().all(Option::is_some));
}
