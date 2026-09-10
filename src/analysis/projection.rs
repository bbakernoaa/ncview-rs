use std::{collections::HashMap, num::NonZeroUsize};

use kiddo::{ImmutableKdTree, SquaredEuclidean};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceIndex {
    pub row: usize,
    pub col: usize,
}

pub struct ProjectionIndex {
    tree: ImmutableKdTree<f64, 2>,
    points: Vec<([f64; 2], SourceIndex)>,
}

impl ProjectionIndex {
    pub fn build(lat: &[f64], lon: &[f64], cols: usize) -> Self {
        let mut points: Vec<([f64; 2], SourceIndex)> = Vec::new();
        let mut point_indices: HashMap<(u64, u64), usize> = HashMap::new();
        for (index, (&latitude, &longitude)) in lat.iter().zip(lon).enumerate() {
            if latitude.is_finite() && longitude.is_finite() {
                let source = SourceIndex {
                    row: index / cols,
                    col: index % cols,
                };
                let normalized = normalize_longitude(longitude);
                for longitude in [normalized, normalized - 360.0, normalized + 360.0] {
                    let coordinate = [latitude, longitude];
                    let key = (latitude.to_bits(), longitude.to_bits());
                    if let Some(existing) = point_indices.get(&key).copied() {
                        if source < points[existing].1 {
                            points[existing].1 = source;
                        }
                    } else {
                        point_indices.insert(key, points.len());
                        points.push((coordinate, source));
                    }
                }
            }
        }
        let coords: Vec<[f64; 2]> = points.iter().map(|(point, _)| *point).collect();
        let tree = ImmutableKdTree::new_from_slice(&coords).unwrap_or_else(|_| {
            ImmutableKdTree::new_from_slice(&[[0.0, 0.0]]).expect("non-empty fallback")
        });
        Self { tree, points }
    }

    pub fn nearest(&self, latitude: f64, longitude: f64) -> Option<SourceIndex> {
        let query = [latitude, normalize_longitude(longitude)];
        let count = NonZeroUsize::new(self.points.len().clamp(1, 64))?;
        self.tree
            .query(&query)
            .nearest_n::<SquaredEuclidean<f64>>(count)
            .execute()
            .into_iter()
            .filter_map(|candidate| {
                self.points
                    .get(candidate.item as usize)
                    .map(|(_, source)| (candidate.distance, *source))
            })
            .min_by(
                |(distance_left, source_left), (distance_right, source_right)| {
                    distance_left.total_cmp(distance_right).then_with(|| {
                        (source_left.row, source_left.col)
                            .cmp(&(source_right.row, source_right.col))
                    })
                },
            )
            .map(|(_, source)| source)
    }
}

pub fn normalize_longitude(value: f64) -> f64 {
    (value + 180.0).rem_euclid(360.0) - 180.0
}
