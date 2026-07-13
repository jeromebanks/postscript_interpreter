//! The Type 1 charstring interpreter — Stage 6 task 6's core.
//!
//! Charstrings are a tiny stack bytecode for outlines. This is the
//! classic hint-ignoring first pass: stem/hint commands clear their
//! arguments and paint nothing (tiny-skia's antialiasing does the work
//! hints did on 300dpi printers), flex (OtherSubr 0–2) becomes its two
//! literal curves, and OtherSubr 3 (hint replacement) is threaded
//! through faithfully so the following `pop callsubr` sequence works.
//!
//! Output is glyph-space path commands plus the hsbw width; the caller
//! maps them through FontMatrix∘CTM exactly like every other glyph
//! source.

use crate::error::PsError;

/// Glyph-space outline commands (charspace units, typically 1000/em).
pub(crate) enum Cmd {
    Move(f64, f64),
    Line(f64, f64),
    Curve(f64, f64, f64, f64, f64, f64),
    Close,
}

pub(crate) struct Glyph {
    pub cmds: Vec<Cmd>,
    /// hsbw/sbw advance width, glyph space.
    pub width: f64,
}

/// Everything a charstring needs from its font: decrypted subroutines
/// and (for seac) a way to fetch sibling charstrings by name.
pub(crate) struct FontData<'a> {
    pub subrs: &'a [Vec<u8>],
    pub charstring_by_name: &'a dyn Fn(&str) -> Option<Vec<u8>>,
}

const CALL_DEPTH_LIMIT: usize = 30;

pub(crate) fn run(code: &[u8], font: &FontData) -> Result<Glyph, PsError> {
    let mut exec = Exec {
        stack: Vec::new(),
        ps_stack: Vec::new(),
        x: 0.0,
        y: 0.0,
        sbx: 0.0,
        width: 0.0,
        cmds: Vec::new(),
        offset: (0.0, 0.0),
        flex: None,
        done: false,
    };
    exec.run(code, font, 0)?;
    Ok(Glyph {
        cmds: exec.cmds,
        width: exec.width,
    })
}

struct Exec {
    stack: Vec<f64>,
    /// Values OtherSubrs hand back for `pop` (12 17).
    ps_stack: Vec<f64>,
    x: f64,
    y: f64,
    sbx: f64,
    width: f64,
    cmds: Vec<Cmd>,
    /// seac renders the accent shifted by this.
    offset: (f64, f64),
    /// Some(points) while inside a flex sequence: rmovetos accumulate
    /// here instead of emitting moves.
    flex: Option<Vec<(f64, f64)>>,
    done: bool,
}

impl Exec {
    fn push_num(&mut self, v: f64) -> Result<(), PsError> {
        if self.stack.len() >= 48 {
            return Err(PsError::InvalidFont);
        }
        self.stack.push(v);
        Ok(())
    }

    fn moved(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
        if let Some(pts) = &mut self.flex {
            pts.push((self.x, self.y));
        } else {
            self.cmds
                .push(Cmd::Move(self.x + self.offset.0, self.y + self.offset.1));
        }
    }

    fn line(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
        self.cmds
            .push(Cmd::Line(self.x + self.offset.0, self.y + self.offset.1));
    }

    fn curve(&mut self, d: [f64; 6]) {
        let c1 = (self.x + d[0], self.y + d[1]);
        let c2 = (c1.0 + d[2], c1.1 + d[3]);
        self.x = c2.0 + d[4];
        self.y = c2.1 + d[5];
        let (ox, oy) = self.offset;
        self.cmds.push(Cmd::Curve(
            c1.0 + ox,
            c1.1 + oy,
            c2.0 + ox,
            c2.1 + oy,
            self.x + ox,
            self.y + oy,
        ));
    }

    fn take<const N: usize>(&mut self) -> Result<[f64; N], PsError> {
        if self.stack.len() < N {
            return Err(PsError::InvalidFont);
        }
        let at = self.stack.len() - N;
        let mut out = [0.0; N];
        out.copy_from_slice(&self.stack[at..]);
        self.stack.truncate(at);
        Ok(out)
    }

    fn run(&mut self, code: &[u8], font: &FontData, depth: usize) -> Result<(), PsError> {
        if depth > CALL_DEPTH_LIMIT {
            return Err(PsError::InvalidFont);
        }
        let mut i = 0;
        while i < code.len() && !self.done {
            let v = code[i];
            i += 1;
            match v {
                // --- numbers -------------------------------------------
                32..=246 => self.push_num(f64::from(i32::from(v) - 139))?,
                247..=250 => {
                    let w = *code.get(i).ok_or(PsError::InvalidFont)?;
                    i += 1;
                    self.push_num(f64::from((i32::from(v) - 247) * 256 + i32::from(w) + 108))?;
                }
                251..=254 => {
                    let w = *code.get(i).ok_or(PsError::InvalidFont)?;
                    i += 1;
                    self.push_num(f64::from(-(i32::from(v) - 251) * 256 - i32::from(w) - 108))?;
                }
                255 => {
                    let b = code.get(i..i + 4).ok_or(PsError::InvalidFont)?;
                    i += 4;
                    self.push_num(f64::from(i32::from_be_bytes([b[0], b[1], b[2], b[3]])))?;
                }
                // --- hints: ignored, arguments cleared -----------------
                1 | 3 => self.stack.clear(),
                // --- moves ---------------------------------------------
                21 => {
                    let [dx, dy] = self.take()?;
                    self.moved(dx, dy);
                    self.stack.clear();
                }
                22 => {
                    let [dx] = self.take()?;
                    self.moved(dx, 0.0);
                    self.stack.clear();
                }
                4 => {
                    let [dy] = self.take()?;
                    self.moved(0.0, dy);
                    self.stack.clear();
                }
                // --- lines ---------------------------------------------
                5 => {
                    let [dx, dy] = self.take()?;
                    self.line(dx, dy);
                    self.stack.clear();
                }
                6 => {
                    let [dx] = self.take()?;
                    self.line(dx, 0.0);
                    self.stack.clear();
                }
                7 => {
                    let [dy] = self.take()?;
                    self.line(0.0, dy);
                    self.stack.clear();
                }
                // --- curves --------------------------------------------
                8 => {
                    let d = self.take::<6>()?;
                    self.curve(d);
                    self.stack.clear();
                }
                30 => {
                    let [dy1, dx2, dy2, dx3] = self.take()?;
                    self.curve([0.0, dy1, dx2, dy2, dx3, 0.0]);
                    self.stack.clear();
                }
                31 => {
                    let [dx1, dx2, dy2, dy3] = self.take()?;
                    self.curve([dx1, 0.0, dx2, dy2, 0.0, dy3]);
                    self.stack.clear();
                }
                9 => {
                    self.cmds.push(Cmd::Close);
                    self.stack.clear();
                }
                // --- control -------------------------------------------
                10 => {
                    let [n] = self.take()?;
                    let sub = font
                        .subrs
                        .get(n as usize)
                        .ok_or(PsError::InvalidFont)?
                        .clone();
                    self.run(&sub, font, depth + 1)?;
                }
                11 => return Ok(()),
                13 => {
                    let [sbx, wx] = self.take()?;
                    self.sbx = sbx;
                    self.width = wx;
                    self.x = sbx;
                    self.y = 0.0;
                    self.stack.clear();
                }
                14 => {
                    self.done = true;
                }
                // --- escapes -------------------------------------------
                12 => {
                    let e = *code.get(i).ok_or(PsError::InvalidFont)?;
                    i += 1;
                    self.escape(e, font, depth)?;
                }
                _ => return Err(PsError::InvalidFont),
            }
        }
        Ok(())
    }

    fn escape(&mut self, e: u8, font: &FontData, depth: usize) -> Result<(), PsError> {
        match e {
            // dotsection / vstem3 / hstem3: hint machinery, ignored.
            0..=2 => self.stack.clear(),
            // seac: composite glyph from two StandardEncoding charstrings.
            6 => {
                let [asb, adx, ady, bchar, achar] = self.take()?;
                self.stack.clear();
                let composite_sbx = self.sbx;
                let std = crate::encodings::standard_encoding();
                let mut render = |code: f64, offset: (f64, f64)| -> Result<(), PsError> {
                    let name = std
                        .get(code as usize)
                        .copied()
                        .ok_or(PsError::InvalidFont)?;
                    let cs = (font.charstring_by_name)(name).ok_or(PsError::InvalidFont)?;
                    let mut child = Exec {
                        stack: Vec::new(),
                        ps_stack: Vec::new(),
                        x: 0.0,
                        y: 0.0,
                        sbx: 0.0,
                        width: 0.0,
                        cmds: std::mem::take(&mut self.cmds),
                        offset,
                        flex: None,
                        done: false,
                    };
                    child.run(&cs, font, depth + 1)?;
                    self.cmds = child.cmds;
                    Ok(())
                };
                render(bchar, (0.0, 0.0))?;
                // The accent shifts by the composite's own sidebearing
                // minus the accent's standard sidebearing (FreeType's
                // reading of the spec).
                render(achar, (composite_sbx - asb + adx, ady))?;
                self.done = true;
            }
            // sbw: the vertical-writing form of hsbw.
            7 => {
                let [sbx, sby, wx, _wy] = self.take()?;
                self.sbx = sbx;
                self.width = wx;
                self.x = sbx;
                self.y = sby;
                self.stack.clear();
            }
            12 => {
                let [a, b] = self.take()?;
                if b == 0.0 {
                    return Err(PsError::InvalidFont);
                }
                self.push_num(a / b)?;
            }
            // callothersubr: flex (0-2), hint replacement (3), unknown.
            16 => {
                let [n_args, which] = self.take()?;
                let n = n_args as usize;
                if self.stack.len() < n {
                    return Err(PsError::InvalidFont);
                }
                let at = self.stack.len() - n;
                let args: Vec<f64> = self.stack.split_off(at);
                match which as i32 {
                    1 => self.flex = Some(Vec::new()),
                    2 => {}
                    0 => {
                        let pts = self.flex.take().ok_or(PsError::InvalidFont)?;
                        // Seven accumulated points: a reference midpoint
                        // then two cubics' worth.
                        if pts.len() != 7 {
                            return Err(PsError::InvalidFont);
                        }
                        let (ox, oy) = self.offset;
                        self.cmds.push(Cmd::Curve(
                            pts[1].0 + ox,
                            pts[1].1 + oy,
                            pts[2].0 + ox,
                            pts[2].1 + oy,
                            pts[3].0 + ox,
                            pts[3].1 + oy,
                        ));
                        self.cmds.push(Cmd::Curve(
                            pts[4].0 + ox,
                            pts[4].1 + oy,
                            pts[5].0 + ox,
                            pts[5].1 + oy,
                            pts[6].0 + ox,
                            pts[6].1 + oy,
                        ));
                        (self.x, self.y) = pts[6];
                        // The charstring follows with `pop pop
                        // setcurrentpoint`: hand the endpoint back.
                        self.ps_stack.push(self.y);
                        self.ps_stack.push(self.x);
                    }
                    3 => {
                        // Hint replacement: hand the subr number back
                        // for the `pop callsubr` that follows.
                        self.ps_stack.push(args.first().copied().unwrap_or(3.0));
                    }
                    _ => {
                        // Unknown OtherSubr: its args become the pop
                        // results, best-effort.
                        self.ps_stack.extend(args.into_iter().rev());
                    }
                }
            }
            17 => {
                let v = self.ps_stack.pop().unwrap_or(0.0);
                self.push_num(v)?;
            }
            33 => {
                let [x, y] = self.take()?;
                self.x = x;
                self.y = y;
                self.stack.clear();
            }
            _ => return Err(PsError::InvalidFont),
        }
        Ok(())
    }
}
