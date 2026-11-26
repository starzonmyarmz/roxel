//! Command parameters: what a manifest declares, and the values a run gets.
//! Every run sees every declared param, so a missing or ill-typed value falls
//! back to the param's default and an out-of-range one is clamped.

use super::Param;
use crate::grid::Color8;

#[derive(Clone, Debug, PartialEq)]
pub struct ParamSpec {
    pub id: String,
    pub label: String,
    pub kind: ParamKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParamKind {
    Number {
        default: f64,
        min: Option<f64>,
        max: Option<f64>,
    },
    Int {
        default: i32,
        min: Option<i32>,
        max: Option<i32>,
    },
    Bool {
        default: bool,
    },
    Color {
        default: Color8,
    },
    Choice {
        options: Vec<String>,
        default: usize,
    },
}

impl ParamSpec {
    pub fn default_value(&self) -> Param {
        match &self.kind {
            ParamKind::Number { default, .. } => Param::Number(*default),
            ParamKind::Int { default, .. } => Param::Number(f64::from(*default)),
            ParamKind::Bool { default } => Param::Bool(*default),
            ParamKind::Color { default } => Param::Color(*default),
            ParamKind::Choice { options, default } => Param::Text(options[*default].clone()),
        }
    }

    /// `given` made valid for this param: clamped into range, rounded for an
    /// int, or the default when it's missing, the wrong type or not an option.
    pub fn resolve(&self, given: Option<&Param>) -> Param {
        let resolved = match (&self.kind, given) {
            (ParamKind::Number { min, max, .. }, Some(Param::Number(n))) if n.is_finite() => {
                Some(Param::Number(clamp(*n, *min, *max)))
            }
            (ParamKind::Int { min, max, .. }, Some(Param::Number(n))) if n.is_finite() => {
                let min = min.map(f64::from).unwrap_or(f64::from(i32::MIN));
                let max = max.map(f64::from).unwrap_or(f64::from(i32::MAX));
                Some(Param::Number(n.round().clamp(min, max)))
            }
            (ParamKind::Bool { .. }, Some(b @ Param::Bool(_))) => Some(b.clone()),
            (ParamKind::Color { .. }, Some(c @ Param::Color(_))) => Some(c.clone()),
            (ParamKind::Choice { options, .. }, Some(Param::Text(s))) if options.contains(s) => {
                Some(Param::Text(s.clone()))
            }
            _ => None,
        };
        resolved.unwrap_or_else(|| self.default_value())
    }
}

/// One value per declared param, in declaration order. Values for params the
/// command doesn't declare are dropped.
pub fn resolve_params(specs: &[ParamSpec], given: &[(String, Param)]) -> Vec<(String, Param)> {
    specs
        .iter()
        .map(|spec| {
            let value = given.iter().find(|(id, _)| *id == spec.id).map(|(_, v)| v);
            (spec.id.clone(), spec.resolve(value))
        })
        .collect()
}

fn clamp(n: f64, min: Option<f64>, max: Option<f64>) -> f64 {
    let n = min.map_or(n, |m| n.max(m));
    max.map_or(n, |m| n.min(m))
}

/// The default when a manifest leaves it out: 0, pulled into range.
pub(super) fn zero_in_range<T: PartialOrd + Copy + Default>(min: Option<T>, max: Option<T>) -> T {
    let mut v = T::default();
    if let Some(m) = min
        && v < m
    {
        v = m;
    }
    if let Some(m) = max
        && v > m
    {
        v = m;
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: ParamKind) -> ParamSpec {
        ParamSpec {
            id: "p".into(),
            label: "P".into(),
            kind,
        }
    }

    #[test]
    fn numbers_clamp_into_range() {
        let s = spec(ParamKind::Number {
            default: 1.0,
            min: Some(0.0),
            max: Some(2.0),
        });
        assert_eq!(s.resolve(Some(&Param::Number(5.0))), Param::Number(2.0));
        assert_eq!(s.resolve(Some(&Param::Number(-1.0))), Param::Number(0.0));
        assert_eq!(s.resolve(Some(&Param::Number(0.5))), Param::Number(0.5));
        assert_eq!(
            s.resolve(Some(&Param::Number(f64::NAN))),
            Param::Number(1.0)
        );
        assert_eq!(s.resolve(None), Param::Number(1.0));
    }

    #[test]
    fn ints_round_and_clamp() {
        let s = spec(ParamKind::Int {
            default: 3,
            min: Some(1),
            max: None,
        });
        assert_eq!(s.resolve(Some(&Param::Number(2.6))), Param::Number(3.0));
        assert_eq!(s.resolve(Some(&Param::Number(-4.0))), Param::Number(1.0));
        assert_eq!(
            s.resolve(Some(&Param::Number(1e12))),
            Param::Number(f64::from(i32::MAX))
        );
    }

    #[test]
    fn wrong_types_and_unknown_choices_fall_back_to_the_default() {
        let s = spec(ParamKind::Choice {
            options: vec!["oak".into(), "pine".into()],
            default: 1,
        });
        assert_eq!(
            s.resolve(Some(&Param::Text("oak".into()))),
            Param::Text("oak".into())
        );
        assert_eq!(
            s.resolve(Some(&Param::Text("elm".into()))),
            Param::Text("pine".into())
        );
        assert_eq!(
            s.resolve(Some(&Param::Bool(true))),
            Param::Text("pine".into())
        );
        let b = spec(ParamKind::Bool { default: true });
        assert_eq!(b.resolve(Some(&Param::Number(0.0))), Param::Bool(true));
        let c = spec(ParamKind::Color {
            default: [1, 2, 3, 4],
        });
        assert_eq!(c.resolve(Some(&Param::Color([9; 4]))), Param::Color([9; 4]));
        assert_eq!(
            c.resolve(Some(&Param::Text("red".into()))),
            Param::Color([1, 2, 3, 4])
        );
    }

    #[test]
    fn resolve_params_fills_every_declared_param_in_order() {
        let specs = [
            ParamSpec {
                id: "a".into(),
                label: "A".into(),
                kind: ParamKind::Bool { default: false },
            },
            ParamSpec {
                id: "b".into(),
                label: "B".into(),
                kind: ParamKind::Int {
                    default: 2,
                    min: None,
                    max: None,
                },
            },
        ];
        let given = [
            ("b".to_string(), Param::Number(7.0)),
            ("stray".to_string(), Param::Bool(true)),
        ];
        assert_eq!(
            resolve_params(&specs, &given),
            [
                ("a".to_string(), Param::Bool(false)),
                ("b".to_string(), Param::Number(7.0)),
            ]
        );
    }

    #[test]
    fn zero_in_range_pulls_zero_into_the_range() {
        assert_eq!(zero_in_range::<i32>(None, None), 0);
        assert_eq!(zero_in_range(Some(2), Some(9)), 2);
        assert_eq!(zero_in_range(None, Some(-3.0)), -3.0);
    }
}
