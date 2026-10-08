//! Helpers shared by unstructured-grid formats: intermediate grid spacing and sphere nearest-point lookup.

use std::num::NonZeroUsize;

use kiddo::{ImmutableKdTree, SquaredEuclidean};

use crate::error::{NcvError, Result};

/// Cell size of the intermediate regular grid, in degrees.
pub const INTERMEDIATE_SPACING_ENV: &str = "NCVIEW_UG_INTERMEDIATE_SPACING_DEG";
pub const DEFAULT_INTERMEDIATE_SPACING_DEG: f64 = 0.25;

/// Parse the intermediate grid spacing; `None` or a blank value selects the default.
pub fn parse_intermediate_spacing(value: Option<&str>) -> Result<f64> {
    let Some(text) = value.map(str::trim).filter(|text| !text.is_empty()) else {
        return Ok(DEFAULT_INTERMEDIATE_SPACING_DEG);
    };
    match text.parse::<f64>() {
        Ok(spacing) if spacing.is_finite() && spacing > 0.0 && spacing <= 90.0 => Ok(spacing),
        _ => Err(NcvError::InvalidRange(format!(
            "{INTERMEDIATE_SPACING_ENV}={text} must be a spacing in degrees within (0, 90]"
        ))),
    }
}

fn unit_vector(latitude_deg: f64, longitude_deg: f64) -> [f64; 3] {
    let latitude = latitude_deg.to_radians();
    let longitude = longitude_deg.to_radians();
    [
        latitude.cos() * longitude.cos(),
        latitude.cos() * longitude.sin(),
        latitude.sin(),
    ]
}

/// Nearest-point lookup on the unit sphere, so distances stay true near the poles.
/// Queries farther than the mesh's local spacing from every mesh point return `None`.
pub struct SphereIndex {
    tree: ImmutableKdTree<f64, 3>,
    sources: Vec<usize>,
    mesh_length: usize,
    reach_sq: f64,
}

const REACH_NEIGHBOURS: usize = 4;

impl SphereIndex {
    pub fn build(lat: &[f64], lon: &[f64]) -> Self {
        let mut points = Vec::new();
        let mut sources = Vec::new();
        for (index, (&latitude, &longitude)) in lat.iter().zip(lon).enumerate() {
            if latitude.is_finite() && longitude.is_finite() {
                points.push(unit_vector(latitude, longitude));
                sources.push(index);
            }
        }
        let tree = ImmutableKdTree::new_from_slice(&points).unwrap_or_else(|_| {
            ImmutableKdTree::new_from_slice(&[[0.0, 0.0, 1.0]]).expect("non-empty fallback")
        });
        let reach_sq = match NonZeroUsize::new(points.len().min(REACH_NEIGHBOURS)) {
            Some(count) if points.len() > 1 => points
                .iter()
                .map(|point| {
                    tree.query(point)
                        .nearest_n::<SquaredEuclidean<f64>>(count)
                        .execute()
                        .into_iter()
                        .last()
                        .map_or(0.0, |candidate| candidate.distance)
                })
                .fold(0.0, f64::max),
            _ => f64::INFINITY,
        };
        Self {
            tree,
            sources,
            mesh_length: lat.len(),
            reach_sq,
        }
    }

    pub fn mesh_length(&self) -> usize {
        self.mesh_length
    }

    pub fn nearest(&self, latitude: f64, longitude: f64) -> Option<usize> {
        let query = unit_vector(latitude, longitude);
        let count = NonZeroUsize::new(1)?;
        let candidate = self
            .tree
            .query(&query)
            .nearest_n::<SquaredEuclidean<f64>>(count)
            .execute()
            .into_iter()
            .next()?;
        if candidate.distance > self.reach_sq {
            return None;
        }
        self.sources.get(candidate.item as usize).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_defaults_when_unset_or_blank() {
        assert_eq!(
            parse_intermediate_spacing(None).unwrap(),
            DEFAULT_INTERMEDIATE_SPACING_DEG
        );
        assert_eq!(
            parse_intermediate_spacing(Some("  ")).unwrap(),
            DEFAULT_INTERMEDIATE_SPACING_DEG
        );
    }

    #[test]
    fn spacing_accepts_values_within_zero_to_ninety_degrees() {
        assert_eq!(parse_intermediate_spacing(Some("0.5")).unwrap(), 0.5);
        assert_eq!(parse_intermediate_spacing(Some(" 90 ")).unwrap(), 90.0);
    }

    #[test]
    fn spacing_rejects_non_positive_non_finite_and_oversized_values() {
        for value in ["0", "-1", "NaN", "inf", "abc", "91"] {
            assert!(
                parse_intermediate_spacing(Some(value)).is_err(),
                "{value} should be rejected"
            );
        }
    }

    #[test]
    fn sphere_nearest_prefers_the_close_point_across_the_pole() {
        // Degree distance picks (80, 90), but on the sphere (89, 0) is about 1.4 degrees away.
        let index = SphereIndex::build(&[80.0, 89.0], &[90.0, 0.0]);
        assert_eq!(index.nearest(89.0, 90.0), Some(1));
    }

    #[test]
    fn sphere_nearest_skips_non_finite_mesh_points() {
        let index = SphereIndex::build(&[f64::NAN, 10.0], &[0.0, 0.0]);
        assert_eq!(index.nearest(10.0, 0.0), Some(1));
        assert_eq!(SphereIndex::build(&[], &[]).nearest(0.0, 0.0), None);
    }

    #[test]
    fn sphere_index_reports_the_full_mesh_length() {
        let index = SphereIndex::build(&[f64::NAN, 10.0], &[0.0, 0.0]);
        assert_eq!(index.mesh_length(), 2);
    }

    #[test]
    fn sphere_nearest_masks_points_beyond_the_mesh_reach() {
        // A 1-degree cluster sets the reach; a query 40 degrees away has no nearby mesh point.
        let index = SphereIndex::build(&[0.0, 0.0, 1.0, 1.0], &[0.0, 1.0, 0.0, 1.0]);
        assert!(index.nearest(0.5, 0.5).is_some());
        assert_eq!(index.nearest(40.0, 40.0), None);
    }

    #[test]
    fn sphere_nearest_keeps_every_point_of_a_global_mesh() {
        let mut lat = Vec::new();
        let mut lon = Vec::new();
        for row in 0..18 {
            for col in 0..36 {
                lat.push(-85.0 + 10.0 * f64::from(row));
                lon.push(10.0 * f64::from(col));
            }
        }
        let index = SphereIndex::build(&lat, &lon);
        for (latitude, longitude) in [(0.0, 5.0), (-89.9, 0.0), (89.9, 179.0), (45.0, 355.0)] {
            assert!(
                index.nearest(latitude, longitude).is_some(),
                "({latitude}, {longitude})"
            );
        }
    }
}
