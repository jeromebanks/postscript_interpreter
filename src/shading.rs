//! Shading dictionaries for the `sh` operator (issue #20) — the engine;
//! `ops/shading.rs` is the operand-stack adapter.
//!
//! Scoped to `ShadingType` 2 (axial) and 3 (radial), `ColorSpace`
//! `/DeviceGray`/`/DeviceRGB`/`/DeviceCMYK`, and `FunctionType` 2
//! (exponential interpolation) and 3 (stitching — what makes
//! multi-stop gradients possible). That combination is deliberate, not
//! just the easy 20%: Types 2 and 3 are pure arithmetic over dict
//! contents, so evaluating them never needs to run a PostScript
//! procedure through the machine — unlike `FunctionType` 0 (sampled)
//! or 4 (PostScript calculator), or `Separation`'s tint transform,
//! which all need the `Frame::PostOp` continuation pattern
//! (`ops/color.rs`) because they *do* execute program-supplied code.
//! Staying inside 2/3 keeps this fully synchronous. Documented gaps:
//! `FunctionType` 0/4, an array of one-in-one-out functions in place
//! of a single N-out function, and Indexed/Separation as a shading's
//! `ColorSpace`.
//!
//! A shading's color ramp is pre-sampled into a fixed list of stops
//! (`Shading::stops`) rather than evaluated per-pixel: `Gfx::sh`
//! hands them straight to a tiny-skia gradient shader, which already
//! does its own linear interpolation between stops — exact for the
//! common N=1 (linear) case, a close approximation for other `N` or
//! for stitched multi-leg functions. Sampling walks each stitching
//! leg's own subdomain separately (`sample_positions`) so a hard
//! color-stop boundary between two legs lands exactly on a stop
//! instead of smearing across a sample interval.

use crate::error::PsError;
use crate::object::{Dict, Value};

/// Samples per stitching leg (or total, for a non-stitched function).
const SAMPLES_PER_LEG: usize = 48;

#[derive(Clone)]
pub(crate) enum PsFunction {
    /// `FunctionType` 2: `C0 + x^N * (C1 - C0)`, x clamped to `domain`.
    Exponential {
        domain: (f64, f64),
        c0: Vec<f64>,
        c1: Vec<f64>,
        n: f64,
    },
    /// `FunctionType` 3: `functions[i]` covers the subdomain between
    /// `bounds[i-1]` and `bounds[i]` (domain edges at the ends),
    /// remapped through `encode[i]` before being evaluated.
    Stitching {
        domain: (f64, f64),
        functions: Vec<PsFunction>,
        bounds: Vec<f64>,
        encode: Vec<(f64, f64)>,
    },
}

impl PsFunction {
    fn ncomp(&self) -> usize {
        match self {
            PsFunction::Exponential { c0, .. } => c0.len(),
            PsFunction::Stitching { functions, .. } => {
                functions.first().map(PsFunction::ncomp).unwrap_or(0)
            }
        }
    }

    fn eval(&self, x: f64) -> Vec<f64> {
        match self {
            PsFunction::Exponential { domain, c0, c1, n } => {
                let x = x.clamp(domain.0.min(domain.1), domain.0.max(domain.1));
                let xn = x.powf(*n);
                c0.iter().zip(c1).map(|(a, b)| a + xn * (b - a)).collect()
            }
            PsFunction::Stitching {
                domain,
                functions,
                bounds,
                encode,
            } => {
                let x = x.clamp(domain.0, domain.1);
                let (idx, lo, hi) = stitch_index(x, *domain, bounds);
                let (e0, e1) = encode[idx];
                let xe = if hi > lo {
                    e0 + (x - lo) * (e1 - e0) / (hi - lo)
                } else {
                    e0
                };
                functions[idx].eval(xe)
            }
        }
    }

    /// Every t-value (in this function's own domain) worth sampling —
    /// every stitching leg's endpoints included, so hard color-stop
    /// boundaries land exactly on a stop instead of being smeared.
    fn sample_positions(&self) -> Vec<f64> {
        match self {
            PsFunction::Exponential { domain, .. } => linspace(domain.0, domain.1, SAMPLES_PER_LEG),
            PsFunction::Stitching { domain, bounds, .. } => {
                let mut edges = Vec::with_capacity(bounds.len() + 2);
                edges.push(domain.0);
                edges.extend(bounds.iter().copied());
                edges.push(domain.1);
                let mut out = Vec::new();
                for w in edges.windows(2) {
                    out.extend(linspace(w[0], w[1], SAMPLES_PER_LEG));
                }
                out
            }
        }
    }
}

fn linspace(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    if n <= 1 {
        return vec![lo];
    }
    (0..n)
        .map(|i| lo + (hi - lo) * (i as f64 / (n - 1) as f64))
        .collect()
}

/// Which leg of a stitching function covers `x`, plus that leg's own
/// subdomain bounds — `bounds` is assumed already validated (in range,
/// non-decreasing) by `parse_function`.
fn stitch_index(x: f64, domain: (f64, f64), bounds: &[f64]) -> (usize, f64, f64) {
    let mut lo = domain.0;
    for (i, &b) in bounds.iter().enumerate() {
        if x < b {
            return (i, lo, b);
        }
        lo = b;
    }
    (bounds.len(), lo, domain.1)
}

#[derive(Clone, Copy)]
enum CsKind {
    Gray,
    Rgb,
    Cmyk,
}

impl CsKind {
    fn ncomp(self) -> usize {
        match self {
            CsKind::Gray => 1,
            CsKind::Rgb => 3,
            CsKind::Cmyk => 4,
        }
    }

    fn to_rgb(self, c: &[f64]) -> (f64, f64, f64) {
        match self {
            CsKind::Gray => (c[0], c[0], c[0]),
            CsKind::Rgb => (c[0], c[1], c[2]),
            CsKind::Cmyk => {
                let (cy, m, y, k) = (c[0], c[1], c[2], c[3]);
                (
                    (1.0 - cy) * (1.0 - k),
                    (1.0 - m) * (1.0 - k),
                    (1.0 - y) * (1.0 - k),
                )
            }
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum ShadingKind {
    Axial {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
    },
    Radial {
        x0: f64,
        y0: f64,
        r0: f64,
        x1: f64,
        y1: f64,
        r1: f64,
    },
}

/// A parsed, ready-to-paint shading: geometry plus a pre-sampled color
/// ramp (`(position 0..1, r, g, b)`, all already resolved to RGB).
/// `/Extend` is validated for shape but not otherwise honored — `sh`
/// always extends both ends (documented in `Gfx::sh`).
pub(crate) struct Shading {
    pub(crate) kind: ShadingKind,
    pub(crate) stops: Vec<(f32, f32, f32, f32)>,
}

fn get(d: &Dict, key: &str) -> Result<crate::object::Object, PsError> {
    d.get(key).ok_or(PsError::Rangecheck)
}

fn get_num(d: &Dict, key: &str) -> Result<f64, PsError> {
    match get(d, key)?.value {
        Value::Integer(n) => Ok(n as f64),
        Value::Real(r) => Ok(r),
        _ => Err(PsError::Typecheck),
    }
}

fn get_int(d: &Dict, key: &str) -> Result<i64, PsError> {
    match get(d, key)?.value {
        Value::Integer(n) => Ok(n),
        _ => Err(PsError::Typecheck),
    }
}

fn get_farray(d: &Dict, key: &str) -> Result<Vec<f64>, PsError> {
    let Value::Array(a) = get(d, key)?.value else {
        return Err(PsError::Typecheck);
    };
    (0..a.len())
        .map(|i| match a.get(i).map(|o| o.value) {
            Some(Value::Integer(n)) => Ok(n as f64),
            Some(Value::Real(r)) => Ok(r),
            _ => Err(PsError::Typecheck),
        })
        .collect()
}

fn get_domain(d: &Dict) -> Result<(f64, f64), PsError> {
    match d.get("Domain") {
        None => Ok((0.0, 1.0)),
        Some(_) => {
            let dm = get_farray(d, "Domain")?;
            match dm.as_slice() {
                [lo, hi] => Ok((*lo, *hi)),
                _ => Err(PsError::Rangecheck),
            }
        }
    }
}

fn parse_function(obj: &crate::object::Object) -> Result<PsFunction, PsError> {
    let Value::Dict(dr) = &obj.value else {
        return Err(PsError::Typecheck);
    };
    let d = dr.borrow();
    let domain = get_domain(&d)?;
    match get_int(&d, "FunctionType")? {
        2 => {
            let c0 = get_farray(&d, "C0")?;
            let c1 = get_farray(&d, "C1")?;
            if c0.is_empty() || c0.len() != c1.len() {
                return Err(PsError::Rangecheck);
            }
            let n = d
                .get("N")
                .map(|_| get_num(&d, "N"))
                .transpose()?
                .unwrap_or(1.0);
            Ok(PsFunction::Exponential { domain, c0, c1, n })
        }
        3 => {
            let Value::Array(fa) = get(&d, "Functions")?.value else {
                return Err(PsError::Typecheck);
            };
            if fa.is_empty() {
                return Err(PsError::Rangecheck);
            }
            let functions: Vec<PsFunction> = (0..fa.len())
                .map(|i| parse_function(&fa.get(i).expect("len checked")))
                .collect::<Result<_, _>>()?;
            let ncomp = functions[0].ncomp();
            if functions.iter().any(|f| f.ncomp() != ncomp) {
                return Err(PsError::Rangecheck);
            }
            let bounds = get_farray(&d, "Bounds")?;
            if bounds.len() != functions.len() - 1 {
                return Err(PsError::Rangecheck);
            }
            // Bounds must be non-decreasing and inside Domain — a
            // malformed dict here would otherwise reach stitch_index's
            // unguarded functions[idx]/encode[idx] with an index the
            // Functions/Bounds shape check alone can't rule out.
            let mut prev = domain.0;
            for &b in &bounds {
                if b < prev || b > domain.1 {
                    return Err(PsError::Rangecheck);
                }
                prev = b;
            }
            let encode_flat = get_farray(&d, "Encode")?;
            if encode_flat.len() != functions.len() * 2 {
                return Err(PsError::Rangecheck);
            }
            let encode = encode_flat.chunks(2).map(|c| (c[0], c[1])).collect();
            Ok(PsFunction::Stitching {
                domain,
                functions,
                bounds,
                encode,
            })
        }
        _ => Err(PsError::Rangecheck),
    }
}

fn parse_colorspace(obj: &crate::object::Object) -> Result<CsKind, PsError> {
    let Value::Name(n) = &obj.value else {
        return Err(PsError::Typecheck);
    };
    match &**n {
        "DeviceGray" => Ok(CsKind::Gray),
        "DeviceRGB" => Ok(CsKind::Rgb),
        "DeviceCMYK" => Ok(CsKind::Cmyk),
        _ => Err(PsError::Typecheck),
    }
}

fn build_stops(
    cs: CsKind,
    function: &PsFunction,
    shading_domain: (f64, f64),
) -> Result<Vec<(f32, f32, f32, f32)>, PsError> {
    let (d0, d1) = shading_domain;
    let span = d1 - d0;
    let mut out = Vec::new();
    for s in function.sample_positions() {
        // s is in the *function's* domain; map it back to the
        // shading's 0..1 gradient-position space.
        let pos = if span.abs() > 1e-12 {
            ((s - d0) / span) as f32
        } else {
            0.0
        };
        let comps = function.eval(s);
        if comps.len() != cs.ncomp() {
            return Err(PsError::Rangecheck);
        }
        let (r, g, b) = cs.to_rgb(&comps);
        if !r.is_finite() || !g.is_finite() || !b.is_finite() {
            return Err(PsError::Rangecheck);
        }
        out.push((
            pos.clamp(0.0, 1.0),
            r.clamp(0.0, 1.0) as f32,
            g.clamp(0.0, 1.0) as f32,
            b.clamp(0.0, 1.0) as f32,
        ));
    }
    Ok(out)
}

pub(crate) fn parse_shading_dict(d: &Dict) -> Result<Shading, PsError> {
    let shading_type = get_int(d, "ShadingType")?;
    let cs = parse_colorspace(&get(d, "ColorSpace")?)?;
    let coords = get_farray(d, "Coords")?;
    let function = parse_function(&get(d, "Function")?)?;
    if function.ncomp() != cs.ncomp() {
        return Err(PsError::Rangecheck);
    }
    let shading_domain = match d.get("Domain") {
        None => (0.0, 1.0),
        Some(_) => get_domain(d)?,
    };
    // /Extend, if present, must have the PLRM shape — accepted and
    // validated but not otherwise honored; see Gfx::sh.
    if let Some(ext) = d.get("Extend") {
        let Value::Array(a) = ext.value else {
            return Err(PsError::Typecheck);
        };
        if a.len() != 2
            || !(0..2).all(|i| matches!(a.get(i).map(|o| o.value), Some(Value::Boolean(_))))
        {
            return Err(PsError::Typecheck);
        }
    }
    let kind = match shading_type {
        2 => match coords.as_slice() {
            [x0, y0, x1, y1] => ShadingKind::Axial {
                x0: *x0,
                y0: *y0,
                x1: *x1,
                y1: *y1,
            },
            _ => return Err(PsError::Rangecheck),
        },
        3 => match coords.as_slice() {
            [x0, y0, r0, x1, y1, r1] => {
                if *r0 < 0.0 || *r1 < 0.0 {
                    return Err(PsError::Rangecheck);
                }
                ShadingKind::Radial {
                    x0: *x0,
                    y0: *y0,
                    r0: *r0,
                    x1: *x1,
                    y1: *y1,
                    r1: *r1,
                }
            }
            _ => return Err(PsError::Rangecheck),
        },
        _ => return Err(PsError::Rangecheck),
    };
    let stops = build_stops(cs, &function, shading_domain)?;
    Ok(Shading { kind, stops })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exponential_linear_ramp_is_exact_at_endpoints() {
        let f = PsFunction::Exponential {
            domain: (0.0, 1.0),
            c0: vec![0.0, 0.0, 0.0],
            c1: vec![1.0, 0.5, 0.25],
            n: 1.0,
        };
        assert_eq!(f.eval(0.0), vec![0.0, 0.0, 0.0]);
        assert_eq!(f.eval(1.0), vec![1.0, 0.5, 0.25]);
        assert_eq!(f.eval(0.5), vec![0.5, 0.25, 0.125]);
    }

    #[test]
    fn exponential_clamps_to_domain() {
        let f = PsFunction::Exponential {
            domain: (0.0, 1.0),
            c0: vec![0.0],
            c1: vec![1.0],
            n: 1.0,
        };
        assert_eq!(f.eval(-5.0), vec![0.0]);
        assert_eq!(f.eval(5.0), vec![1.0]);
    }

    #[test]
    fn stitching_picks_the_right_leg_and_remaps() {
        let f = PsFunction::Stitching {
            domain: (0.0, 1.0),
            functions: vec![
                PsFunction::Exponential {
                    domain: (0.0, 1.0),
                    c0: vec![0.0],
                    c1: vec![1.0],
                    n: 1.0,
                },
                PsFunction::Exponential {
                    domain: (0.0, 1.0),
                    c0: vec![1.0],
                    c1: vec![0.0],
                    n: 1.0,
                },
            ],
            bounds: vec![0.5],
            encode: vec![(0.0, 1.0), (0.0, 1.0)],
        };
        // First leg covers [0, 0.5) remapped to [0,1]: at x=0.25 -> 0.5.
        assert_eq!(f.eval(0.25), vec![0.5]);
        // Second leg covers [0.5, 1] remapped to [0,1]: at x=0.75 -> 0.5.
        assert_eq!(f.eval(0.75), vec![0.5]);
        // Exact leg boundary lands on the second leg (x < b test).
        assert_eq!(f.eval(0.5), vec![1.0]);
    }

    #[test]
    fn sample_positions_hit_every_leg_boundary_exactly() {
        let f = PsFunction::Stitching {
            domain: (0.0, 1.0),
            functions: vec![
                PsFunction::Exponential {
                    domain: (0.0, 1.0),
                    c0: vec![0.0],
                    c1: vec![1.0],
                    n: 1.0,
                },
                PsFunction::Exponential {
                    domain: (0.0, 1.0),
                    c0: vec![0.0],
                    c1: vec![1.0],
                    n: 1.0,
                },
            ],
            bounds: vec![0.3],
            encode: vec![(0.0, 1.0), (0.0, 1.0)],
        };
        let pts = f.sample_positions();
        assert!(pts.contains(&0.0));
        assert!(pts.iter().any(|p| (p - 0.3).abs() < 1e-9));
        assert!(pts.contains(&1.0));
        // The boundary appears twice (last of leg 0, first of leg 1).
        assert_eq!(pts.iter().filter(|p| (**p - 0.3).abs() < 1e-9).count(), 2);
    }

    #[test]
    fn colorspace_conversion() {
        assert_eq!(CsKind::Gray.to_rgb(&[0.5]), (0.5, 0.5, 0.5));
        assert_eq!(CsKind::Rgb.to_rgb(&[1.0, 0.5, 0.0]), (1.0, 0.5, 0.0));
        let (r, g, b) = CsKind::Cmyk.to_rgb(&[0.0, 0.0, 0.0, 1.0]);
        assert_eq!((r, g, b), (0.0, 0.0, 0.0));
    }
}
