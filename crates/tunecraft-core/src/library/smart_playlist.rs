use chrono::{NaiveDate, NaiveDateTime};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::database::models::Track;

/// Comparison operator for smart playlist rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Operator {
    Eq,
    Ne,
    Gt,
    Ge,
    Lt,
    Le,
    Contains,
}

/// Dynamic value for rule comparison.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleValue {
    Text(String),
    Integer(i64),
    Float(f64),
    Date(NaiveDate),
    DateTime(NaiveDateTime),
}

/// A single filter rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub field: String,
    pub operator: Operator,
    pub value: RuleValue,
}

impl Rule {
    /// Evaluate this rule against a track.
    pub fn matches(&self, track: &Track) -> bool {
        match (self.operator, &self.value) {
            (Operator::Eq, RuleValue::Text(b)) => {
                if let FieldValue::Text(a) = self.extract_field(track) {
                    a.to_lowercase() == b.to_lowercase()
                } else {
                    false
                }
            }
            (Operator::Ne, RuleValue::Text(b)) => {
                if let FieldValue::Text(a) = self.extract_field(track) {
                    a.to_lowercase() != b.to_lowercase()
                } else {
                    false
                }
            }
            (Operator::Contains, RuleValue::Text(b)) => {
                if let FieldValue::Text(a) = self.extract_field(track) {
                    a.to_lowercase().contains(&b.to_lowercase())
                } else {
                    false
                }
            }

            (Operator::Eq, RuleValue::Integer(b)) => {
                if let FieldValue::Integer(a) = self.extract_field(track) {
                    a == *b
                } else {
                    false
                }
            }
            (Operator::Ne, RuleValue::Integer(b)) => {
                if let FieldValue::Integer(a) = self.extract_field(track) {
                    a != *b
                } else {
                    false
                }
            }
            (Operator::Gt, RuleValue::Integer(b)) => {
                if let FieldValue::Integer(a) = self.extract_field(track) {
                    a > *b
                } else {
                    false
                }
            }
            (Operator::Ge, RuleValue::Integer(b)) => {
                if let FieldValue::Integer(a) = self.extract_field(track) {
                    a >= *b
                } else {
                    false
                }
            }
            (Operator::Lt, RuleValue::Integer(b)) => {
                if let FieldValue::Integer(a) = self.extract_field(track) {
                    a < *b
                } else {
                    false
                }
            }
            (Operator::Le, RuleValue::Integer(b)) => {
                if let FieldValue::Integer(a) = self.extract_field(track) {
                    a <= *b
                } else {
                    false
                }
            }

            (Operator::Eq, RuleValue::Float(b)) => {
                if let FieldValue::Float(a) = self.extract_field(track) {
                    (a - b).abs() < 1e-6
                } else {
                    false
                }
            }
            (Operator::Ne, RuleValue::Float(b)) => {
                if let FieldValue::Float(a) = self.extract_field(track) {
                    (a - b).abs() >= 1e-6
                } else {
                    false
                }
            }
            (Operator::Gt, RuleValue::Float(b)) => {
                if let FieldValue::Float(a) = self.extract_field(track) {
                    a > *b
                } else {
                    false
                }
            }
            (Operator::Ge, RuleValue::Float(b)) => {
                if let FieldValue::Float(a) = self.extract_field(track) {
                    a >= *b
                } else {
                    false
                }
            }
            (Operator::Lt, RuleValue::Float(b)) => {
                if let FieldValue::Float(a) = self.extract_field(track) {
                    a < *b
                } else {
                    false
                }
            }
            (Operator::Le, RuleValue::Float(b)) => {
                if let FieldValue::Float(a) = self.extract_field(track) {
                    a <= *b
                } else {
                    false
                }
            }

            (Operator::Eq, RuleValue::Date(b)) => {
                if let FieldValue::Date(a) = self.extract_field(track) {
                    a == *b
                } else {
                    false
                }
            }
            (Operator::Ne, RuleValue::Date(b)) => {
                if let FieldValue::Date(a) = self.extract_field(track) {
                    a != *b
                } else {
                    false
                }
            }
            (Operator::Gt, RuleValue::Date(b)) => {
                if let FieldValue::Date(a) = self.extract_field(track) {
                    a > *b
                } else {
                    false
                }
            }
            (Operator::Ge, RuleValue::Date(b)) => {
                if let FieldValue::Date(a) = self.extract_field(track) {
                    a >= *b
                } else {
                    false
                }
            }
            (Operator::Lt, RuleValue::Date(b)) => {
                if let FieldValue::Date(a) = self.extract_field(track) {
                    a < *b
                } else {
                    false
                }
            }
            (Operator::Le, RuleValue::Date(b)) => {
                if let FieldValue::Date(a) = self.extract_field(track) {
                    a <= *b
                } else {
                    false
                }
            }

            _ => {
                debug!(
                    "type mismatch in rule {:?}: field={:?}, value={:?}",
                    self,
                    self.extract_field(track),
                    self.value
                );
                false
            }
        }
    }

    fn extract_field(&self, track: &Track) -> FieldValue {
        match self.field.as_str() {
            "title" => FieldValue::Text(track.title.clone().unwrap_or_default()),
            "artist" => FieldValue::Text(track.artist.clone().unwrap_or_default()),
            "album" => FieldValue::Text(track.album.clone().unwrap_or_default()),
            "genre" => FieldValue::Text(track.genre.clone().unwrap_or_default()),
            "year" => FieldValue::Integer(track.year.unwrap_or(0) as i64),
            "duration" => FieldValue::Integer(track.duration.unwrap_or(0) as i64),
            "bitrate" => FieldValue::Integer(track.bitrate.unwrap_or(0) as i64),
            "play_count" => FieldValue::Integer(track.play_count.unwrap_or(0)),
            "skip_count" => FieldValue::Integer(track.skip_count.unwrap_or(0)),
            "rating" => FieldValue::Float(track.rating.unwrap_or(0.0)),
            "date_added" => FieldValue::Date(track.date_added),
            "last_played" => FieldValue::Date(
                track
                    .last_played
                    .unwrap_or_else(|| NaiveDate::from_ymd_opt(1970, 1, 1).unwrap()),
            ),
            "file_path" => FieldValue::Text(track.file_path.clone()),
            "mood" => FieldValue::Text(
                track
                    .mood_override
                    .clone()
                    .or_else(|| track.mood.clone())
                    .unwrap_or_default(),
            ),
            "mood_override" => FieldValue::Text(track.mood_override.clone().unwrap_or_default()),
            "bpm" => FieldValue::Float(track.bpm.unwrap_or(0.0)),
            "energy" => FieldValue::Float(track.energy.unwrap_or(0.0)),
            "bass_ratio" => FieldValue::Float(track.bass_ratio.unwrap_or(0.0)),
            "spectral_centroid" => FieldValue::Float(track.spectral_centroid.unwrap_or(0.0)),
            "dynamic_range" => FieldValue::Float(track.dynamic_range.unwrap_or(0.0)),
            _ => FieldValue::Text(String::new()),
        }
    }
}

/// Extracted field value from a track for rule evaluation.
#[derive(Debug, Clone)]
enum FieldValue {
    Text(String),
    Integer(i64),
    Float(f64),
    Date(NaiveDate),
}

/// Sortable value extracted from a track for proper comparison.
/// Numeric fields are compared numerically, not as strings.
#[derive(Debug, Clone)]
enum SortValue {
    Text(String),
    Number(i64),
    Float(f64),
}

impl SortValue {
    /// Compare two sort values for ordering.
    /// Text values are compared lexicographically; numeric values numerically.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (self, other) {
            (SortValue::Text(a), SortValue::Text(b)) => a.cmp(b),
            (SortValue::Number(a), SortValue::Number(b)) => a.cmp(b),
            (SortValue::Float(a), SortValue::Float(b)) => {
                a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)
            }
            (SortValue::Number(_), SortValue::Float(_)) => std::cmp::Ordering::Less,
            (SortValue::Float(_), SortValue::Number(_)) => std::cmp::Ordering::Greater,
            (SortValue::Number(_), SortValue::Text(_)) => std::cmp::Ordering::Less,
            (SortValue::Text(_), SortValue::Number(_)) => std::cmp::Ordering::Greater,
            (SortValue::Float(_), SortValue::Text(_)) => std::cmp::Ordering::Less,
            (SortValue::Text(_), SortValue::Float(_)) => std::cmp::Ordering::Greater,
        }
    }
}

/// Extract a sortable value from a track for a given field name.
fn extract_sort_value(track: &Track, field: &str) -> SortValue {
    match field {
        "title" => SortValue::Text(track.title.clone().unwrap_or_default()),
        "artist" => SortValue::Text(track.artist.clone().unwrap_or_default()),
        "album" => SortValue::Text(track.album.clone().unwrap_or_default()),
        "year" => SortValue::Number(track.year.unwrap_or(0) as i64),
        "duration" => SortValue::Number(track.duration.unwrap_or(0) as i64),
        "rating" => SortValue::Float(track.rating.unwrap_or(0.0)),
        "play_count" => SortValue::Number(track.play_count.unwrap_or(0)),
        "skip_count" => SortValue::Number(track.skip_count.unwrap_or(0)),
        "date_added" => SortValue::Text(track.date_added.to_string()),
        "bpm" => SortValue::Float(track.bpm.unwrap_or(0.0)),
        "energy" => SortValue::Float(track.energy.unwrap_or(0.0)),
        "dynamic_range" => SortValue::Float(track.dynamic_range.unwrap_or(0.0)),
        "spectral_centroid" => SortValue::Float(track.spectral_centroid.unwrap_or(0.0)),
        "bass_ratio" => SortValue::Float(track.bass_ratio.unwrap_or(0.0)),
        "last_played" => {
            SortValue::Text(track.last_played.map(|d| d.to_string()).unwrap_or_default())
        }
        "file_path" => SortValue::Text(track.file_path.clone()),
        "mood" => SortValue::Text(
            track
                .mood_override
                .clone()
                .or_else(|| track.mood.clone())
                .unwrap_or_default(),
        ),
        "mood_override" => SortValue::Text(track.mood_override.clone().unwrap_or_default()),
        _ => SortValue::Text(String::new()),
    }
}

/// Logical connector for combining rule nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Connector {
    And,
    Or,
}

/// A node in the rule evaluation tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RuleNode {
    Rule(Rule),
    Group {
        connector: Connector,
        children: Vec<RuleNode>,
    },
}

impl RuleNode {
    /// Evaluate this node against a track.
    pub fn matches(&self, track: &Track) -> bool {
        match self {
            RuleNode::Rule(rule) => rule.matches(track),
            RuleNode::Group {
                connector: Connector::And,
                children,
            } => children.iter().all(|c| c.matches(track)),
            RuleNode::Group {
                connector: Connector::Or,
                children,
            } => children.iter().any(|c| c.matches(track)),
        }
    }
}

/// A smart playlist that auto-populates based on rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmartPlaylist {
    pub name: String,
    pub rules: RuleNode,
    pub limit: Option<usize>,
    pub sort_by: Option<String>,
    pub sort_desc: bool,
}

impl SmartPlaylist {
    /// Create a new smart playlist with the given name and rules.
    pub fn new(name: impl Into<String>, rules: RuleNode) -> Self {
        Self {
            name: name.into(),
            rules,
            limit: None,
            sort_by: None,
            sort_desc: false,
        }
    }

    /// Compile this playlist's rules into an executable filter function.
    pub fn compile(&self) -> impl Fn(&Track) -> bool + '_ {
        move |track: &Track| self.rules.matches(track)
    }

    /// Execute this smart playlist against a track collection.
    pub fn execute<'a>(&self, tracks: &'a [Track]) -> Vec<&'a Track> {
        let filter = self.compile();
        let mut result: Vec<_> = tracks.iter().filter(|t| filter(t)).collect();

        if let Some(ref sort_field) = self.sort_by {
            let sort_desc = self.sort_desc;
            result.sort_by(|a, b| {
                let va = extract_sort_value(a, sort_field);
                let vb = extract_sort_value(b, sort_field);
                if sort_desc {
                    vb.cmp(&va)
                } else {
                    va.cmp(&vb)
                }
            });
        }

        if let Some(limit) = self.limit {
            result.truncate(limit);
        }

        result
    }

    /// Set the maximum number of tracks.
    pub fn with_limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Set the sort field and direction.
    pub fn with_sort(mut self, field: impl Into<String>, desc: bool) -> Self {
        self.sort_by = Some(field.into());
        self.sort_desc = desc;
        self
    }
}

/// Pre-built smart playlist templates.
pub mod templates {
    use super::*;

    /// Recently added tracks (last 30 days).
    pub fn recently_added() -> SmartPlaylist {
        SmartPlaylist::new(
            "Recently Added",
            RuleNode::Rule(Rule {
                field: "date_added".into(),
                operator: Operator::Gt,
                value: RuleValue::Date(
                    chrono::Local::now().date_naive() - chrono::Duration::days(30),
                ),
            }),
        )
        .with_limit(50)
        .with_sort("date_added", true)
    }

    /// Most played tracks.
    pub fn most_played() -> SmartPlaylist {
        SmartPlaylist::new(
            "Most Played",
            RuleNode::Rule(Rule {
                field: "play_count".into(),
                operator: Operator::Gt,
                value: RuleValue::Integer(0),
            }),
        )
        .with_limit(50)
        .with_sort("play_count", true)
    }

    /// Favorites (highly rated tracks — 3+ stars on a 0–5 scale).
    pub fn favorites() -> SmartPlaylist {
        SmartPlaylist::new(
            "Favorites",
            RuleNode::Rule(Rule {
                field: "rating".into(),
                operator: Operator::Ge,
                value: RuleValue::Float(3.0),
            }),
        )
        .with_limit(100)
        .with_sort("rating", true)
    }

    /// Tracks by a specific artist.
    pub fn by_artist(artist: &str) -> SmartPlaylist {
        SmartPlaylist::new(
            format!("By {}", artist),
            RuleNode::Rule(Rule {
                field: "artist".into(),
                operator: Operator::Eq,
                value: RuleValue::Text(artist.to_string()),
            }),
        )
    }

    /// Genre filter.
    pub fn by_genre(genre: &str) -> SmartPlaylist {
        SmartPlaylist::new(
            format!("Genre: {}", genre),
            RuleNode::Rule(Rule {
                field: "genre".into(),
                operator: Operator::Contains,
                value: RuleValue::Text(genre.to_string()),
            }),
        )
    }

    /// Recently played (has plays, sorted by last_played date descending).
    pub fn recently_played() -> SmartPlaylist {
        SmartPlaylist::new(
            "Recently Played",
            RuleNode::Rule(Rule {
                field: "play_count".into(),
                operator: Operator::Gt,
                value: RuleValue::Integer(0),
            }),
        )
        .with_limit(30)
        .with_sort("last_played", true)
    }

    /// Filter tracks by a specific mood category.
    pub fn by_mood(mood: &str) -> SmartPlaylist {
        SmartPlaylist::new(
            format!("Mood: {}", mood),
            RuleNode::Rule(Rule {
                field: "mood".into(),
                operator: Operator::Eq,
                value: RuleValue::Text(mood.to_string()),
            }),
        )
        .with_limit(100)
        .with_sort("rating", true)
    }

    /// High-energy dance tracks (BPM > 120).
    pub fn high_energy() -> SmartPlaylist {
        SmartPlaylist::new(
            "High Energy",
            RuleNode::Rule(Rule {
                field: "bpm".into(),
                operator: Operator::Gt,
                value: RuleValue::Float(120.0),
            }),
        )
        .with_limit(50)
        .with_sort("bpm", true)
    }

    /// Low-energy chill tracks (BPM < 90, energy < 0.05).
    pub fn low_energy() -> SmartPlaylist {
        SmartPlaylist::new(
            "Low Energy",
            RuleNode::Group {
                connector: Connector::And,
                children: vec![
                    RuleNode::Rule(Rule {
                        field: "bpm".into(),
                        operator: Operator::Lt,
                        value: RuleValue::Float(90.0),
                    }),
                    RuleNode::Rule(Rule {
                        field: "energy".into(),
                        operator: Operator::Lt,
                        value: RuleValue::Float(0.05),
                    }),
                ],
            },
        )
        .with_limit(50)
        .with_sort("energy", false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::models::Track;
    use chrono::NaiveDate;

    fn make_test_track() -> Track {
        Track {
            id: Some(1),
            file_path: "/music/test.mp3".into(),
            file_hash: Some("abc123".into()),
            file_size: Some(3000000),
            file_mtime: Some(1700000000),
            title: Some("Test Song".into()),
            artist: Some("Test Artist".into()),
            album: Some("Test Album".into()),
            genre: Some("Rock".into()),
            year: Some(2023),
            track_number: Some(1),
            duration: Some(240),
            sample_rate: Some(44100),
            bitrate: Some(320),
            play_count: Some(5),
            skip_count: Some(1),
            rating: Some(4.0),
            love: None,
            bpm: Some(120.0),
            energy: Some(0.08),
            bass_ratio: Some(0.30),
            spectral_centroid: Some(2500.0),
            dynamic_range: Some(0.05),
            mood: Some("Dance".into()),
            mood_override: None,
            date_added: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
            last_played: None,
        }
    }

    #[test]
    fn test_empty_rule_group_and() {
        let node = RuleNode::Group {
            connector: Connector::And,
            children: vec![],
        };
        let track = make_test_track();
        assert!(
            node.matches(&track),
            "Empty AND group should match (vacuous truth)"
        );
    }

    #[test]
    fn test_empty_rule_group_or() {
        let node = RuleNode::Group {
            connector: Connector::Or,
            children: vec![],
        };
        let track = make_test_track();
        assert!(!node.matches(&track), "Empty OR group should not match");
    }

    #[test]
    fn test_nested_groups() {
        let inner_and = RuleNode::Group {
            connector: Connector::And,
            children: vec![
                RuleNode::Rule(Rule {
                    field: "genre".into(),
                    operator: Operator::Eq,
                    value: RuleValue::Text("Rock".into()),
                }),
                RuleNode::Rule(Rule {
                    field: "year".into(),
                    operator: Operator::Ge,
                    value: RuleValue::Integer(2020),
                }),
            ],
        };
        let outer_or = RuleNode::Group {
            connector: Connector::Or,
            children: vec![
                inner_and,
                RuleNode::Rule(Rule {
                    field: "mood".into(),
                    operator: Operator::Eq,
                    value: RuleValue::Text("Dance".into()),
                }),
            ],
        };
        let track = make_test_track();
        assert!(
            outer_or.matches(&track),
            "Track should match: (Rock AND year>=2020) OR mood=Dance"
        );
    }

    #[test]
    fn test_unknown_field_returns_empty_text() {
        let rule = Rule {
            field: "nonexistent_field".into(),
            operator: Operator::Eq,
            value: RuleValue::Text("anything".into()),
        };
        let track = make_test_track();
        assert!(!rule.matches(&track), "Unknown field should not match");
    }

    #[test]
    fn test_type_mismatch_returns_false() {
        let rule = Rule {
            field: "artist".into(),
            operator: Operator::Gt,
            value: RuleValue::Integer(42),
        };
        let track = make_test_track();
        assert!(!rule.matches(&track), "Type mismatch should not match");
    }

    #[test]
    fn test_smart_playlist_with_limit() {
        let playlist = SmartPlaylist::new(
            "Test",
            RuleNode::Rule(Rule {
                field: "genre".into(),
                operator: Operator::Contains,
                value: RuleValue::Text("Rock".into()),
            }),
        )
        .with_limit(1);

        let tracks = vec![make_test_track(), make_test_track()];
        let result = playlist.execute(&tracks);
        assert_eq!(result.len(), 1, "Limit should be applied");
    }

    #[test]
    fn test_smart_playlist_with_sort() {
        let playlist = SmartPlaylist::new(
            "Test",
            RuleNode::Rule(Rule {
                field: "play_count".into(),
                operator: Operator::Gt,
                value: RuleValue::Integer(0),
            }),
        )
        .with_sort("play_count", true);

        let mut t1 = make_test_track();
        t1.play_count = Some(5);
        t1.title = Some("Low plays".into());
        let mut t2 = make_test_track();
        t2.play_count = Some(20);
        t2.title = Some("High plays".into());

        let result = playlist.execute(&[t1, t2]);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].play_count, Some(20));
        assert_eq!(result[1].play_count, Some(5));
    }

    #[test]
    fn test_date_comparison() {
        let rule = Rule {
            field: "date_added".into(),
            operator: Operator::Gt,
            value: RuleValue::Date(NaiveDate::from_ymd_opt(2023, 6, 1).unwrap()),
        };
        let track = make_test_track();
        assert!(
            rule.matches(&track),
            "Track added 2024-01-01 should be after 2023-06-01"
        );
    }

    #[test]
    fn test_mood_override_priority() {
        let mut track = make_test_track();
        track.mood = Some("Dance".into());
        track.mood_override = Some("Chill".into());
        let rule = Rule {
            field: "mood".into(),
            operator: Operator::Eq,
            value: RuleValue::Text("Chill".into()),
        };
        assert!(
            rule.matches(&track),
            "mood field should use mood_override when present"
        );
    }

    #[test]
    fn test_float_field_comparison() {
        let rule = Rule {
            field: "bpm".into(),
            operator: Operator::Gt,
            value: RuleValue::Float(100.0),
        };
        let track = make_test_track();
        assert!(rule.matches(&track), "Track BPM=120 should be > 100");
    }
}
