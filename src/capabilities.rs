//! Machine-readable catalog of agent-usable art capabilities (issue #39).
//!
//! `--capabilities` (CLI) and `describe_art_capabilities` (`pscat-mcp`)
//! both serialize [`payload_json`] — a single list covering fonts, the
//! Type 3 program faces (`lib/fonts/`, plus `lib/handscript.ps`'s
//! `/HandScript` and `lib/hangul.ps`'s `/HangulScript` — two historical
//! outliers that predate the `lib/fonts/` convention), artkit's mood
//! palettes, the page templates (`lib/pagekit.ps`), and artkit's/the
//! style packs'/handscript's/hangul's/paintkit's major procedures. It
//! exists so an agent can discover what's
//! *actually installed* in the running `pscat` without embedding
//! prose documentation (which drifts — `psart`'s `SKILL.md` had
//! already fallen behind artkit's paragraph-flow, hyperbolic-geometry,
//! noise/flow, and gradient sections by the time this catalog was
//! built) in its prompt context.
//!
//! ## Design
//!
//! Fonts are enumerated live from [`crate::font::catalog_entries`] —
//! there's no static font list here to drift from what `findfont` can
//! actually reach. Everything else (Type 3 faces, palettes, templates,
//! procedures) is PostScript source with no docstring convention this
//! module could parse, so their metadata — description, calling
//! convention, source file — is hand-maintained in [`ENTRIES`] below.
//! What keeps *that* from silently drifting is `tests/capabilities.rs`,
//! which actually loads each `.ps` source into a real `Interp` and
//! checks the name set both ways: every cataloged name must still be
//! defined there, and every name the file defines must be either
//! cataloged or listed in one of the `INTERNAL_*` allowlists below
//! (scratch helpers and internal state, not part of the public API).
//! **Registering a new capability**: add an [`Entry`] to the matching
//! section of [`ENTRIES`]; if it's a deliberately private helper
//! instead, add its name to the relevant `INTERNAL_*` list. Either way
//! `cargo test capabilities` fails until one of those is done, so a
//! newly added public name in `lib/artkit.ps`/`lib/styles/*`/
//! `lib/pagekit.ps` can't ship silently uncataloged.
//!
//! Most procedures take positional PostScript stack arguments, not
//! named ones — there's nothing here as clean as a template's
//! content-dict keys to model as [`Param`]s, so a procedure's calling
//! convention is carried in `example` (the stack-effect comment
//! already written at its definition site) instead, and `parameters`
//! stays empty. Templates, and the two options-dict procedures
//! `hs-write`/`hs-linecount`/`hg-write`/`hg-linecount` (whose calling
//! convention genuinely *is* one dict with named, defaulted keys —
//! same shape as a template's content dict), use `parameters` for
//! real.
//!
//! `example` alone isn't enough to actually run something, though — a
//! page template errors `undefined: Palettes` unless `lib/artkit.ps`
//! is loaded first, and that dependency isn't visible from `source`
//! alone (a gap a cross-model review caught on the first PR, #74). Each
//! entry's `load` field carries the exact `run` sequence needed before
//! `example` will work, derived from `(kind, source)` in
//! [`load_sequence`] rather than duplicated per entry.

use serde_json::{Value, json};

use crate::font;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CapabilityKind {
    Font,
    Type3Face,
    Palette,
    Template,
    Procedure,
    /// A mutable global config variable (e.g. `/spmetal`) — not
    /// executable, just a name bound to a value that other procedures
    /// in the same source read. Distinct from `Procedure` (a fourth
    /// cross-model review finding, PR #74): a consumer enumerating
    /// `kind == "procedure"` and calling every entry would find `/name
    /// exec` on a dial only pushes its value, not runs anything.
    Dial,
}

impl CapabilityKind {
    fn as_str(self) -> &'static str {
        match self {
            CapabilityKind::Font => "font",
            CapabilityKind::Type3Face => "type3_face",
            CapabilityKind::Palette => "palette",
            CapabilityKind::Template => "template",
            CapabilityKind::Procedure => "procedure",
            CapabilityKind::Dial => "dial",
        }
    }
}

#[derive(Clone, Copy)]
pub struct Param {
    pub name: &'static str,
    pub description: &'static str,
    pub default: Option<&'static str>,
}

impl Param {
    fn to_json(self) -> Value {
        json!({
            "name": self.name,
            "description": self.description,
            "default": self.default,
        })
    }
}

/// One catalog row, owned so fonts (built at call time from
/// [`font::catalog_entries`]) and the static [`ENTRIES`] table can
/// share the same output shape.
pub struct Capability {
    pub name: String,
    pub kind: CapabilityKind,
    pub description: String,
    pub parameters: Vec<Param>,
    pub source: String,
    /// The `run` sequence(s) needed to get `source` into scope before
    /// `example` will work — e.g. `(lib/artkit.ps) run
    /// (lib/pagekit.ps) run` for a page template, since loading
    /// `pagekit.ps` alone errors `undefined: Palettes` (a real gap a
    /// cross-model review caught: PR #74). Empty for fonts, which need
    /// no `run` at all.
    pub load: String,
    pub example: String,
    pub availability: String,
}

impl Capability {
    fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "kind": self.kind.as_str(),
            "description": self.description,
            "parameters": self.parameters.iter().map(|p| p.to_json()).collect::<Vec<_>>(),
            "source": self.source,
            "load": self.load,
            "example": self.example,
            "availability": self.availability,
        })
    }
}

/// The `run` sequence needed to get `source` into scope, given `kind`.
/// A page template or style-pack procedure needs `lib/artkit.ps`
/// loaded first (it leans on `Palettes`/`pal`/`tfblock`/etc.); a Type 3
/// face just needs its own file. Fonts and font aliases need no `run`
/// at all (`findfont` resolves them directly).
fn load_sequence(kind: CapabilityKind, source: &str) -> String {
    match kind {
        CapabilityKind::Font => String::new(),
        CapabilityKind::Type3Face => format!("({source}) run"),
        _ => match source {
            s if s == ARTKIT => "(lib/artkit.ps) run".to_string(),
            s if s == PAGEKIT => "(lib/artkit.ps) run (lib/pagekit.ps) run".to_string(),
            s if s == PAINTKIT => "(lib/artkit.ps) run (lib/paintkit.ps) run".to_string(),
            s if s == STEAMPUNK => "(lib/artkit.ps) run (lib/styles/steampunk.ps) run".to_string(),
            s if s == PSYCHEDELIC => {
                "(lib/artkit.ps) run (lib/styles/psychedelic.ps) run".to_string()
            }
            s if s == SCIFI => "(lib/artkit.ps) run (lib/styles/scifi.ps) run".to_string(),
            s if s == TOON => "(lib/artkit.ps) run (lib/styles/toon.ps) run".to_string(),
            s if s == HANDSCRIPT => "(lib/handscript.ps) run".to_string(),
            s if s == HANGUL => "(lib/hangul.ps) run".to_string(),
            _ => String::new(),
        },
    }
}

/// The hand-maintained rows: everything except fonts (see module docs
/// for why). `&'static` throughout — no allocation until a [`Capability`]
/// is actually built from one.
struct Entry {
    name: &'static str,
    kind: CapabilityKind,
    description: &'static str,
    parameters: &'static [Param],
    source: &'static str,
    example: &'static str,
    availability: &'static str,
}

impl Entry {
    fn to_capability(&self) -> Capability {
        Capability {
            name: self.name.to_string(),
            kind: self.kind,
            description: self.description.to_string(),
            parameters: self.parameters.to_vec(),
            source: self.source.to_string(),
            load: load_sequence(self.kind, self.source),
            example: self.example.to_string(),
            availability: self.availability.to_string(),
        }
    }
}

const ARTKIT: &str = "lib/artkit.ps";
const PAGEKIT: &str = "lib/pagekit.ps";
const PAINTKIT: &str = "lib/paintkit.ps";
const STEAMPUNK: &str = "lib/styles/steampunk.ps";
const PSYCHEDELIC: &str = "lib/styles/psychedelic.ps";
const SCIFI: &str = "lib/styles/scifi.ps";
const TOON: &str = "lib/styles/toon.ps";
const HANDSCRIPT: &str = "lib/handscript.ps";
const HANGUL: &str = "lib/hangul.ps";
const LIB: &str = "library"; // availability: always present once its source is loaded

/// Every top-level name `lib/artkit.ps` defines that is *not* cataloged
/// as a `Procedure` below — internal scratch helpers (`ap-`, `tf-`,
/// `n-` prefixed and friends) plus two dicts (`Palettes`, `TurtleState`)
/// that are data, not procedures. `tests/capabilities.rs` uses this
/// list (plus [`ENTRIES`]'s artkit procedure names) as the full
/// expected top-level name set for `lib/artkit.ps` alone, and as the
/// baseline to subtract when checking what each style pack adds on
/// top of it.
pub const ARTKIT_INTERNAL: &[&str] = &[
    "alintern",
    "apseg",
    "wkmeasure",
    "wkseg",
    "wkend",
    "gethash",
    "grad2",
    "Grad2",
    "htmax",
    "tflinebreak",
    "tfwordgaps",
    "tfdrawline",
    "FractalGens",
    "nfade",
    "nlerp",
    "Palettes",
    "TurtleState",
];

/// Top-level names `lib/pagekit.ps` defines beyond its five templates:
/// the three shared helpers (`pggetdef`/`pgzfitmax`/`pgframe`).
pub const PAGEKIT_INTERNAL: &[&str] = &["pggetdef", "pgzfitmax", "pgframe"];

/// Top-level names `lib/paintkit.ps` defines beyond `pkribbon`/`pknib`/
/// `pkdry`/`pkspray` and the three `/Pressure` presets (`pkflat`/
/// `pktaper`/`pkbell`, cataloged separately since they're documented,
/// directly referenceable by name): the dict-default reader, the
/// internal ribbon-construction helpers (edge offsetting, cap/loop
/// building, taper/half-width math), `pknib`'s own travel-angle
/// sampling helpers (issue #42), `pkdry`'s per-bristle dash-run helper
/// and clamped-probability roll (issue #43), and `pkspray`'s particle
/// generator, burst-cluster helper, and clamped-probability roll
/// (issue #44).
pub const PAINTKIT_INTERNAL: &[&str] = &[
    "pkgetdef",
    "pktaperf",
    "pkhalfwat",
    "pkjit",
    "pkedge",
    "pkdot",
    "pkloop",
    "pkopenrun",
    "pkbuildrun",
    "pkscanclosed",
    "pbroll",
    "pnangleat",
    "pnpressure",
    "pbdashrun",
    "pzpdot",
    "pzburstcluster",
    "pzroll",
    "poroll",
    "porad",
];

/// Top-level names `lib/handscript.ps` defines beyond `hs-write`/
/// `hs-linecount`: `HandScriptDict` (the `definefont` template dict —
/// the face itself, `/HandScript`, lives in `FontDirectory`, not
/// `userdict`) and `HSLayout` (internal line-wrap state).
pub const HANDSCRIPT_INTERNAL: &[&str] = &["HandScriptDict", "HSLayout"];

/// Top-level names `lib/hangul.ps` defines beyond `hg-write`/
/// `hg-linecount`: `HangulDict` (the `definefont` template dict —
/// `/HangulScript` itself lives in `FontDirectory`) and `HGLayout`
/// (internal line-wrap state).
pub const HANGUL_INTERNAL: &[&str] = &["HangulDict", "HGLayout"];

macro_rules! entry {
    ($name:expr, $kind:expr, $desc:expr, $params:expr, $source:expr, $example:expr, $avail:expr) => {
        Entry {
            name: $name,
            kind: $kind,
            description: $desc,
            parameters: $params,
            source: $source,
            example: $example,
            availability: $avail,
        }
    };
}

static ENTRIES: &[Entry] = &[
    // --- Type 3 program faces (lib/fonts/, Stages 15 & 18) ---------
    entry!(
        "Neon",
        CapabilityKind::Type3Face,
        "Letters as bent glass tube: each stroke drawn five times, wide-and-dim to thin-and-overdriven, for a pure-overdraw glow. Best on a near-black ground.",
        &[],
        "lib/fonts/neon.ps",
        "/Neon findfont 90 scalefont setfont",
        LIB
    ),
    entry!(
        "Marquee",
        CapabilityKind::Type3Face,
        "Letters as a theater marquee sign: warm incandescent bulbs along every stroke, a few burnt out (rand-driven). Best on a dark ground.",
        &[],
        "lib/fonts/marquee.ps",
        "/Marquee findfont 90 scalefont setfont",
        LIB
    ),
    entry!(
        "Constellation",
        CapabilityKind::Type3Face,
        "Every letter is a constellation: named stars at the skeleton's anchor points, hairline asterism lines, a scatter of field stars (rand-driven). Meant for a midnight ground.",
        &[],
        "lib/fonts/constellation.ps",
        "/Constellation findfont 110 scalefont setfont",
        LIB
    ),
    entry!(
        "Lapidary",
        CapabilityKind::Type3Face,
        "Capitals cut into stone: drop shadow, raised face, incised groove, sunlit arris. Deterministic, no rand. Meant for pale grounds.",
        &[],
        "lib/fonts/lapidary.ps",
        "/Lapidary findfont 90 scalefont setfont",
        LIB
    ),
    entry!(
        "Circuitry",
        CapabilityKind::Type3Face,
        "Letters as copper PCB traces: solder-mask channel, copper trace, specular seam, through-hole pads at terminals, vias along long runs. Best on solder-mask green or midnight-blue.",
        &[],
        "lib/fonts/circuitry.ps",
        "/Circuitry findfont 90 scalefont setfont",
        LIB
    ),
    entry!(
        "Stitchwork",
        CapabilityKind::Type3Face,
        "Letters embroidered in cross-stitch: every stroke walked at even pitch, each stop an X on the +-45deg grid, three thread passes per leg. Lovely on linen, cream, or kraft.",
        &[],
        "lib/fonts/stitchwork.ps",
        "/Stitchwork findfont 90 scalefont setfont",
        LIB
    ),
    entry!(
        "Confetti",
        CapabilityKind::Type3Face,
        "Letters spelled by thrown confetti: tilted paper slips and round dots in party colors, dense at the letterform, feathering outward. Works on light or dark grounds.",
        &[],
        "lib/fonts/confetti.ps",
        "/Confetti findfont 110 scalefont setfont",
        LIB
    ),
    entry!(
        "HandScript",
        CapabilityKind::Type3Face,
        "Dynamic handwriting face: every glyph generated fresh with rand-driven jitter, so repeated characters never render identically. Lowercase Latin only (the font has no caps). Backs the `handwrite` tool/scripts/handwrite.sh; also directly callable via hs-write/hs-linecount below.",
        &[],
        HANDSCRIPT,
        "/HandScript findfont 36 scalefont setfont",
        LIB
    ),
    entry!(
        "HangulScript",
        CapabilityKind::Type3Face,
        "Procedural jittered-stroke Hangul face: composes each syllable from ~14 atomic jamo stroke shapes at BuildChar time (Unicode-mode Type 3 -- pscat-only, see FONTS.md). Modern Hangul only (U+AC00-U+D7A3); any other codepoint (ASCII letters, spaces, punctuation) gets a half-width advance and draws nothing -- a dedicated Hangul face, not mixed-script. Call via hg-write/hg-linecount below, not plain show, for correct word-wrapping.",
        &[],
        HANGUL,
        "/HangulScript findfont 48 scalefont setfont",
        LIB
    ),
    // --- Palettes: artkit's eight moods -----------------------------
    entry!(
        "dusk",
        CapabilityKind::Palette,
        "Twilight: deep indigo through plum to a warm amber glow.",
        &[],
        ARTKIT,
        "/dusk palpick",
        LIB
    ),
    entry!(
        "ember",
        CapabilityKind::Palette,
        "Fire embers: near-black through brick red to bright gold.",
        &[],
        ARTKIT,
        "/ember palpick",
        LIB
    ),
    entry!(
        "tide",
        CapabilityKind::Palette,
        "Ocean shallows: deep teal through sea green to pale foam.",
        &[],
        ARTKIT,
        "/tide palpick",
        LIB
    ),
    entry!(
        "meadow",
        CapabilityKind::Palette,
        "Field and flower: olive through grass green to a rose-pink accent.",
        &[],
        ARTKIT,
        "/meadow palpick",
        LIB
    ),
    entry!(
        "carnival",
        CapabilityKind::Palette,
        "Saturated hue wheel (red/gold/blue/green/violet) — no lightness order.",
        &[],
        ARTKIT,
        "/carnival palpick",
        LIB
    ),
    entry!(
        "parchment",
        CapabilityKind::Palette,
        "Aged paper: cream through tan to a dark sepia ink — runs light to dark.",
        &[],
        ARTKIT,
        "/parchment palpick",
        LIB
    ),
    entry!(
        "nocturne",
        CapabilityKind::Palette,
        "Midnight neon: near-black through electric cyan to hot magenta.",
        &[],
        ARTKIT,
        "/nocturne palpick",
        LIB
    ),
    entry!(
        "stone",
        CapabilityKind::Palette,
        "Neutral greys, charcoal to near-white.",
        &[],
        ARTKIT,
        "/stone palpick",
        LIB
    ),
    // --- Palettes: pagekit's two (Stage/issue #18) ------------------
    entry!(
        "vellum",
        CapabilityKind::Palette,
        "Formal cream, dark-to-light (0 ink / 1 body-dark / 2 body / 3 trim / 4 background).",
        &[],
        PAGEKIT,
        "/vellum palpick",
        LIB
    ),
    entry!(
        "marigold",
        CapabilityKind::Palette,
        "Festive warm, dark-to-light (0 ink / 1 body-dark / 2 body / 3 trim / 4 background).",
        &[],
        PAGEKIT,
        "/marigold palpick",
        LIB
    ),
    // --- Palettes: style packs (Stage 20), dark-to-light, role 0..4 -
    entry!(
        "brass",
        CapabilityKind::Palette,
        "Steampunk brass: dark bronze through warm gold highlight.",
        &[],
        STEAMPUNK,
        "/brass palpick",
        LIB
    ),
    entry!(
        "verdigris",
        CapabilityKind::Palette,
        "Steampunk verdigris: dark oxide green through pale patina.",
        &[],
        STEAMPUNK,
        "/verdigris palpick",
        LIB
    ),
    entry!(
        "boiler",
        CapabilityKind::Palette,
        "Steampunk boiler iron: soot black through rust-orange to steam-white.",
        &[],
        STEAMPUNK,
        "/boiler palpick",
        LIB
    ),
    entry!(
        "acid",
        CapabilityKind::Palette,
        "Psychedelic hot pink/orange/yellow.",
        &[],
        PSYCHEDELIC,
        "/acid palpick",
        LIB
    ),
    entry!(
        "blacklight",
        CapabilityKind::Palette,
        "Psychedelic UV-poster purple/magenta.",
        &[],
        PSYCHEDELIC,
        "/blacklight palpick",
        LIB
    ),
    entry!(
        "sherbet",
        CapabilityKind::Palette,
        "Psychedelic soft pink/peach/cream pastels.",
        &[],
        PSYCHEDELIC,
        "/sherbet palpick",
        LIB
    ),
    entry!(
        "void",
        CapabilityKind::Palette,
        "Sci-fi deep space: near-black through cold blue to ice white.",
        &[],
        SCIFI,
        "/void palpick",
        LIB
    ),
    entry!(
        "hologram",
        CapabilityKind::Palette,
        "Sci-fi cyan hologram: dark teal through bright cyan.",
        &[],
        SCIFI,
        "/hologram palpick",
        LIB
    ),
    entry!(
        "synthwave",
        CapabilityKind::Palette,
        "Sci-fi synthwave: deep purple through hot magenta to warm gold.",
        &[],
        SCIFI,
        "/synthwave palpick",
        LIB
    ),
    entry!(
        "saturday",
        CapabilityKind::Palette,
        "Toon Saturday-morning primaries: red/gold/sky blue.",
        &[],
        TOON,
        "/saturday palpick",
        LIB
    ),
    entry!(
        "latenight",
        CapabilityKind::Palette,
        "Toon muted late-night rerun tones.",
        &[],
        TOON,
        "/latenight palpick",
        LIB
    ),
    entry!(
        "pastelpop",
        CapabilityKind::Palette,
        "Toon soft pastel pink/yellow/mint.",
        &[],
        TOON,
        "/pastelpop palpick",
        LIB
    ),
    // --- Templates: lib/pagekit.ps (issue #18) ----------------------
    entry!(
        "pgcard",
        CapabilityKind::Template,
        "Greeting card front panel: rounded tinted panel, centered title, flowed message, signoff bottom-right.",
        &[
            Param {
                name: "Title",
                description: "Card title, centered",
                default: Some("()")
            },
            Param {
                name: "Body",
                description: "Flowed message body",
                default: Some("()")
            },
            Param {
                name: "Signoff",
                description: "Bottom-right signoff line",
                default: Some("()")
            },
            Param {
                name: "Palette",
                description: "Mood palette name to draw with",
                default: Some("/dusk")
            },
        ],
        PAGEKIT,
        "x y w h << /Title (...) /Body (...) /Signoff (...) >> pgcard  leftover",
        LIB
    ),
    entry!(
        "pgletter",
        CapabilityKind::Template,
        "Plain business/personal letter: header rule, date top-right, salutation, flowed body, reserved signature space.",
        &[
            Param {
                name: "Date",
                description: "Date line, top-right",
                default: Some("()")
            },
            Param {
                name: "Salutation",
                description: "Opening line, top-left",
                default: Some("()")
            },
            Param {
                name: "Body",
                description: "Flowed letter body",
                default: Some("()")
            },
            Param {
                name: "Signoff",
                description: "Closing line above the signature space",
                default: Some("()")
            },
            Param {
                name: "Palette",
                description: "Mood palette name to draw with",
                default: Some("/stone")
            },
        ],
        PAGEKIT,
        "x y w h << /Date (...) /Salutation (...) /Body (...) /Signoff (...) >> pgletter  leftover",
        LIB
    ),
    entry!(
        "pgcertificate",
        CapabilityKind::Template,
        "Formal award: cream background inside a double rule, title, \"this certifies that\" caption, fitted awardee name, flowed citation, presenter/date signature line pair.",
        &[
            Param {
                name: "Title",
                description: "Certificate heading",
                default: Some("(Certificate of Achievement)")
            },
            Param {
                name: "Awardee",
                description: "Recipient name, large and fitted",
                default: Some("()")
            },
            Param {
                name: "Body",
                description: "Flowed citation text",
                default: Some("()")
            },
            Param {
                name: "Presenter",
                description: "Left signature-line label",
                default: Some("()")
            },
            Param {
                name: "Date",
                description: "Right signature-line label",
                default: Some("()")
            },
            Param {
                name: "Palette",
                description: "Mood palette name to draw with",
                default: Some("/vellum")
            },
        ],
        PAGEKIT,
        "x y w h << /Title (...) /Awardee (...) /Body (...) /Presenter (...) /Date (...) >> pgcertificate  leftover",
        LIB
    ),
    entry!(
        "pginvitation",
        CapabilityKind::Template,
        "Event invitation: tinted bordered panel, large event title, centered when/where/host block, flowed RSVP-style body.",
        &[
            Param {
                name: "Title",
                description: "Event title",
                default: Some("()")
            },
            Param {
                name: "When",
                description: "Date/time line",
                default: Some("()")
            },
            Param {
                name: "Where",
                description: "Location line",
                default: Some("()")
            },
            Param {
                name: "Host",
                description: "Host line",
                default: Some("()")
            },
            Param {
                name: "Body",
                description: "Flowed RSVP-style body",
                default: Some("()")
            },
            Param {
                name: "Palette",
                description: "Mood palette name to draw with",
                default: Some("/marigold")
            },
        ],
        PAGEKIT,
        "x y w h << /Title (...) /When (...) /Where (...) /Host (...) /Body (...) >> pginvitation  leftover",
        LIB
    ),
    entry!(
        "pgposter",
        CapabilityKind::Template,
        "Bold single-sheet poster: full-bleed tint, headline sized to fit the width, italic tagline, large flowed body, small footer line.",
        &[
            Param {
                name: "Title",
                description: "Headline, sized to fit the width",
                default: Some("()")
            },
            Param {
                name: "Tagline",
                description: "Italic subhead below the title",
                default: Some("()")
            },
            Param {
                name: "Body",
                description: "Large flowed body text",
                default: Some("()")
            },
            Param {
                name: "Footer",
                description: "Small footer line",
                default: Some("()")
            },
            Param {
                name: "Palette",
                description: "Mood palette name to draw with",
                default: Some("/ember")
            },
        ],
        PAGEKIT,
        "x y w h << /Title (...) /Tagline (...) /Body (...) /Footer (...) >> pgposter  leftover",
        LIB
    ),
    // --- Procedures: lib/artkit.ps, random --------------------------
    entry!(
        "chance",
        CapabilityKind::Procedure,
        "Random integer, seeded by srand.",
        &[],
        ARTKIT,
        "n chance -> int  (0..n-1)",
        LIB
    ),
    entry!(
        "jit",
        CapabilityKind::Procedure,
        "Random integer jitter, seeded by srand.",
        &[],
        ARTKIT,
        "j jit -> int  (-j..j)",
        LIB
    ),
    entry!(
        "frnd",
        CapabilityKind::Procedure,
        "Random real, seeded by srand.",
        &[],
        ARTKIT,
        "frnd -> real  (0..1)",
        LIB
    ),
    entry!(
        "oneof",
        CapabilityKind::Procedure,
        "A random element of an array, seeded by srand.",
        &[],
        ARTKIT,
        "arr oneof -> elem",
        LIB
    ),
    // --- Procedures: color -------------------------------------------
    entry!(
        "mix3",
        CapabilityKind::Procedure,
        "Linear interpolation between two [r g b] colors.",
        &[],
        ARTKIT,
        "[rgb] [rgb] t mix3 -> r g b",
        LIB
    ),
    entry!(
        "shade",
        CapabilityKind::Procedure,
        "Darkens (k<1) or lifts toward white (k>1) an [r g b] color.",
        &[],
        ARTKIT,
        "[rgb] k shade -> r g b",
        LIB
    ),
    entry!(
        "pal",
        CapabilityKind::Procedure,
        "The five-color array for a named mood palette.",
        &[],
        ARTKIT,
        "/name pal -> array",
        LIB
    ),
    entry!(
        "palpick",
        CapabilityKind::Procedure,
        "One random color from a named mood palette.",
        &[],
        ARTKIT,
        "/name palpick -> r g b",
        LIB
    ),
    // --- Procedures: turtle -------------------------------------------
    entry!(
        "thome",
        CapabilityKind::Procedure,
        "Sets the turtle's pose and movetos there.",
        &[],
        ARTKIT,
        "x y heading thome -",
        LIB
    ),
    entry!(
        "fd",
        CapabilityKind::Procedure,
        "Moves the turtle forward, drawing into the current path.",
        &[],
        ARTKIT,
        "dist fd -",
        LIB
    ),
    entry!(
        "hop",
        CapabilityKind::Procedure,
        "Moves the turtle forward, pen up.",
        &[],
        ARTKIT,
        "dist hop -",
        LIB
    ),
    entry!(
        "tl",
        CapabilityKind::Procedure,
        "Turns the turtle left, in degrees.",
        &[],
        ARTKIT,
        "ang tl -",
        LIB
    ),
    entry!(
        "tr",
        CapabilityKind::Procedure,
        "Turns the turtle right, in degrees.",
        &[],
        ARTKIT,
        "ang tr -",
        LIB
    ),
    entry!(
        "tpush",
        CapabilityKind::Procedure,
        "Saves the turtle's pose (depth 64).",
        &[],
        ARTKIT,
        "tpush -",
        LIB
    ),
    entry!(
        "tpop",
        CapabilityKind::Procedure,
        "Restores the turtle's pose.",
        &[],
        ARTKIT,
        "tpop -",
        LIB
    ),
    // --- Procedures: L-systems -----------------------------------------
    entry!(
        "lsys",
        CapabilityKind::Procedure,
        "Expands an L-system axiom under a rule dict to a given depth.",
        &[],
        ARTKIT,
        "(axiom) rules depth lsys -> (expanded)",
        LIB
    ),
    entry!(
        "ldraw",
        CapabilityKind::Procedure,
        "Drives the turtle along an expanded L-system string (F draw, f hop, +/- turn, []  push/pop). Needs a prior thome (fd's first lineto has no current point otherwise).",
        &[],
        ARTKIT,
        "x y heading thome  (str) step angle ldraw -",
        LIB
    ),
    // --- Procedures: brushes -------------------------------------------
    entry!(
        "alongpath",
        CapabilityKind::Procedure,
        "Stamps a proc at even arc-length stops along the current path (charpath text included).",
        &[],
        ARTKIT,
        "pitch proc alongpath -",
        LIB
    ),
    entry!(
        "walkpath",
        CapabilityKind::Procedure,
        "Centerline path sampler: like alongpath, but also hands the proc normalized progress, spacing, and start/end flags per stop -- the foundation for pressure-ribbon and other painterly brushes.",
        &[],
        ARTKIT,
        "pitch proc walkpath -",
        LIB
    ),
    entry!(
        "pathtext",
        CapabilityKind::Procedure,
        "Sets a string along the current path, glyph by glyph, rotated to the tangent.",
        &[],
        ARTKIT,
        "(string) pathtext -",
        LIB
    ),
    entry!(
        "ctext",
        CapabilityKind::Procedure,
        "Sets text along a circle starting at a given angle.",
        &[],
        ARTKIT,
        "cx cy r ang (str) ctext -",
        LIB
    ),
    entry!(
        "ctextctr",
        CapabilityKind::Procedure,
        "Centers text at the top of a circle (ctext's common case).",
        &[],
        ARTKIT,
        "cx cy r (str) ctextctr -",
        LIB
    ),
    // --- Procedures: lib/paintkit.ps (issue #41) ------------------------
    // Unlike everything above, pkribbon takes one options dict with
    // named, defaulted keys -- genuinely Param-shaped, same as a page
    // template's content dict -- so parameters is populated for real.
    entry!(
        "pkribbon",
        CapabilityKind::Procedure,
        "Treats the current path as a centerline and fills a variable-width ribbon along it, built on walkpath. Color comes from whatever the caller already set.",
        &[
            Param {
                name: "Width",
                description: "Base width",
                default: Some("6")
            },
            Param {
                name: "Pitch",
                description: "walkpath sampling pitch",
                default: Some("Width*0.5, capped at 6")
            },
            Param {
                name: "Pressure",
                description: "{t -> mult} proc over normalized path progress (pkflat/pktaper/pkbell presets ship)",
                default: Some("{ pkflat }")
            },
            Param {
                name: "StartTaper",
                description: "0..1 fraction of progress to ramp width in from the start, independent of Pressure",
                default: Some("0")
            },
            Param {
                name: "EndTaper",
                description: "0..1 fraction of progress to ramp width out at the end, independent of Pressure",
                default: Some("0")
            },
            Param {
                name: "StartCap",
                description: "/round, /flat, or /pointed",
                default: Some("/round")
            },
            Param {
                name: "EndCap",
                description: "/round, /flat, or /pointed",
                default: Some("/round")
            },
            Param {
                name: "Jitter",
                description: "Seeded edge displacement amount, deterministic under the caller's N srand",
                default: Some("0")
            },
        ],
        PAINTKIT,
        "newpath ... << /Width 10 /Pressure { pktaper } >> pkribbon",
        LIB
    ),
    entry!(
        "pkflat",
        CapabilityKind::Procedure,
        "pkribbon's default /Pressure preset: constant width (t -> 1.0).",
        &[],
        PAINTKIT,
        "t pkflat mult",
        LIB
    ),
    entry!(
        "pktaper",
        CapabilityKind::Procedure,
        "A pkribbon /Pressure preset: linear taper across the whole stroke (t -> t).",
        &[],
        PAINTKIT,
        "t pktaper mult",
        LIB
    ),
    entry!(
        "pkbell",
        CapabilityKind::Procedure,
        "A pkribbon /Pressure preset: non-linear 0->1->0 hump, widest at the middle of the stroke.",
        &[],
        PAINTKIT,
        "t pkbell mult",
        LIB
    ),
    entry!(
        "pknib",
        CapabilityKind::Procedure,
        "An angled-nib calligraphy preset built on pkribbon: mark width is driven by the angle between local path direction and a fixed nib angle, widest perpendicular to it and narrowing toward MinWidth parallel to it. The current path must be exactly one open subpath -- call once per stroke.",
        &[
            Param {
                name: "Angle",
                description: "Nib angle in degrees; perpendicular to it is widest, parallel narrows toward MinWidth",
                default: Some("45")
            },
            Param {
                name: "MinWidth",
                description: "0..1 floor on the nib-angle response itself, before Pressure and the tapers multiply through",
                default: Some("0.08")
            },
            Param {
                name: "Width",
                description: "Base width, forwarded to pkribbon",
                default: Some("6")
            },
            Param {
                name: "Pitch",
                description: "walkpath sampling pitch, forwarded to pkribbon",
                default: Some("Width*0.5, capped at 6")
            },
            Param {
                name: "Pressure",
                description: "{t -> mult} proc, multiplies with the nib-angle response rather than replacing it",
                default: Some("{ pkflat }")
            },
            Param {
                name: "StartTaper",
                description: "0..1 fraction of progress to ramp width in from the start, forwarded to pkribbon",
                default: Some("0")
            },
            Param {
                name: "EndTaper",
                description: "0..1 fraction of progress to ramp width out at the end, forwarded to pkribbon",
                default: Some("0")
            },
            Param {
                name: "StartCap",
                description: "/round, /flat, or /pointed, forwarded to pkribbon",
                default: Some("/round")
            },
            Param {
                name: "EndCap",
                description: "/round, /flat, or /pointed, forwarded to pkribbon",
                default: Some("/round")
            },
            Param {
                name: "Jitter",
                description: "Seeded edge displacement amount, forwarded to pkribbon",
                default: Some("0")
            },
        ],
        PAINTKIT,
        "newpath ... << /Width 16 /Angle 30 >> pknib",
        LIB
    ),
    entry!(
        "pkdry",
        CapabilityKind::Procedure,
        "A dry-bristle brush built on pkribbon: a bounded family of thin offset bristles scattered across the centerline, each broken into ink/no-ink runs by a seeded two-state Markov chain, ranging from a mostly loaded stroke to visibly broken dry-brush texture with no raster work.",
        &[
            Param {
                name: "Width",
                description: "Envelope width the bristles scatter across",
                default: Some("6")
            },
            Param {
                name: "Bristles",
                description: "Bristle count; hard safety cap 1..100",
                default: Some("18")
            },
            Param {
                name: "Spread",
                description: "0..1 fraction of Width the bristles scatter across, centered on the centerline",
                default: Some("0.85")
            },
            Param {
                name: "BristleWidth",
                description: "Base width of each individual bristle's own mark",
                default: Some("Width*0.12")
            },
            Param {
                name: "WidthJitter",
                description: "0..1 fraction of BristleWidth each bristle's own width randomly varies by",
                default: Some("0.4")
            },
            Param {
                name: "Load",
                description: "Resume-contact rate per one Width of travel along the path",
                default: Some("0.6")
            },
            Param {
                name: "Dropout",
                description: "Lose-contact rate per one Width of travel along the path",
                default: Some("0.4")
            },
            Param {
                name: "Jitter",
                description: "Edge roughness, forwarded verbatim as each dash's own pkribbon Jitter; shares the Markov chain's random stream",
                default: Some("0")
            },
            Param {
                name: "Pitch",
                description: "walkpath sampling pitch for the shared centerline pass",
                default: Some("BristleWidth*0.6, capped at 6")
            },
            Param {
                name: "ColorJitter",
                description: "0..1 magnitude of small per-bristle color variation around the color active when pkdry is called",
                default: Some("0.06")
            },
        ],
        PAINTKIT,
        "newpath ... << /Width 16 /Load 0.9 /Dropout 0.1 >> pkdry",
        LIB
    ),
    entry!(
        "pkspray",
        CapabilityKind::Procedure,
        "A spray-paint brush: seeded opaque particles deposited around each sampled centerline stop, thinning outward under a radial falloff, with an optional overspray mist past the nozzle edge and optional start/end trigger-dwell bursts. Total deposits track arc length (about Density per nozzle-diameter of travel) regardless of Pitch; deterministic under the caller's own srand; respects any active clip, so stencils are just clip before calling.",
        &[
            Param {
                name: "Nozzle",
                description: "Spray radius around the centerline",
                default: Some("8")
            },
            Param {
                name: "Density",
                description: "Mean particles deposited per one nozzle-diameter of travel; uncapped, bounded by the deposit-budget safety limit instead",
                default: Some("30")
            },
            Param {
                name: "Falloff",
                description: "0..1 how quickly density thins outward; three discrete levels (min-of-m-uniforms radial draws, m = 1 + truncate(2*Falloff))",
                default: Some("0.5")
            },
            Param {
                name: "Overspray",
                description: "0..1 probability each particle escapes past the nozzle edge as half-size mist in the band [Nozzle, Nozzle*(1+Overspray)]",
                default: Some("0.12")
            },
            Param {
                name: "Speck",
                description: "Mean particle diameter",
                default: Some("1.4")
            },
            Param {
                name: "Speckle",
                description: "0..1 per-particle diameter variation, clamped to never go below 10% of Speck",
                default: Some("0.35")
            },
            Param {
                name: "Pitch",
                description: "walkpath sampling pitch for the shared centerline pass",
                default: Some("Nozzle*0.5, capped at 6")
            },
            Param {
                name: "StartBurst",
                description: "0..1 extra particles pooled at each subpath's first stop (trigger dwell)",
                default: Some("0")
            },
            Param {
                name: "EndBurst",
                description: "0..1 extra particles pooled at each subpath's last stop (trigger dwell)",
                default: Some("0")
            },
        ],
        PAINTKIT,
        "newpath ... << /Nozzle 12 /Density 40 >> pkspray",
        LIB
    ),
    entry!(
        "pkoil",
        CapabilityKind::Procedure,
        "A stylized oil-paint impasto preset built on pkribbon: layered pressure ribbons and bristle ridges suggesting a loaded brush, with restrained highlight/shadow edge lifts that read as paint thickness without any 3D height map or blending. Deterministic under the caller's own srand; bounded by Ridges * stops; respects clip like every other preset.",
        &[
            Param {
                name: "Width",
                description: "Envelope width the ridges scatter across",
                default: Some("14")
            },
            Param {
                name: "Pitch",
                description: "walkpath sampling pitch for the shared centerline pass",
                default: Some("Width*0.35, capped at 4")
            },
            Param {
                name: "Ridges",
                description: "Bristle ridge count; hard safety cap 1..40",
                default: Some("12")
            },
            Param {
                name: "RidgeSpread",
                description: "0..1 fraction of Width the ridges scatter across, centered on the centerline",
                default: Some("0.75")
            },
            Param {
                name: "RidgeWidth",
                description: "Base width of each ridge's own mark",
                default: Some("Width*0.14")
            },
            Param {
                name: "WidthJitter",
                description: "0..1 fraction of RidgeWidth each ridge's own width randomly varies by",
                default: Some("0.3")
            },
            Param {
                name: "Load",
                description: "Resume-contact rate per one Width of travel; high Load stays inked",
                default: Some("0.85")
            },
            Param {
                name: "Dropout",
                description: "Lose-contact rate per one Width of travel",
                default: Some("0.18")
            },
            Param {
                name: "ColorJitter",
                description: "0..1 per-ridge color variation magnitude around the caller's current color",
                default: Some("0.08")
            },
            Param {
                name: "EdgePickup",
                description: "0..1 darker pickup at the envelope edges",
                default: Some("0.15")
            },
            Param {
                name: "Highlight",
                description: "0..1 restrained highlight strength on one side",
                default: Some("0.12")
            },
            Param {
                name: "Shadow",
                description: "0..1 restrained shadow strength on the opposite side",
                default: Some("0.10")
            },
            Param {
                name: "Pressure",
                description: "{t -> mult} proc over normalized path progress, forwarded to the base ribbon",
                default: Some("{ pkflat }")
            },
            Param {
                name: "Jitter",
                description: "Seeded edge displacement amount, forwarded to each dash's pkribbon Jitter",
                default: Some("0")
            },
        ],
        PAINTKIT,
        "newpath ... << /Width 16 /Ridges 12 /Load 0.9 >> pkoil",
        LIB
    ),
    // --- Procedures: shapes ----------------------------------------------
    entry!(
        "ngon",
        CapabilityKind::Procedure,
        "Appends a regular n-gon to the current path.",
        &[],
        ARTKIT,
        "cx cy r n ngon -",
        LIB
    ),
    entry!(
        "star",
        CapabilityKind::Procedure,
        "Appends an n-pointed star to the current path.",
        &[],
        ARTKIT,
        "cx cy r1 r2 n star -",
        LIB
    ),
    entry!(
        "rrect",
        CapabilityKind::Procedure,
        "Appends a rounded rectangle to the current path.",
        &[],
        ARTKIT,
        "x y w h r rrect -",
        LIB
    ),
    // --- Procedures: text --------------------------------------------------
    entry!(
        "showctr",
        CapabilityKind::Procedure,
        "Shows a string horizontally centered at a point.",
        &[],
        ARTKIT,
        "(s) cx y showctr -",
        LIB
    ),
    entry!(
        "fitfont",
        CapabilityKind::Procedure,
        "Scales currentfont so a string fits a target width (consumes the string).",
        &[],
        ARTKIT,
        "(s) targetwidth fitfont -",
        LIB
    ),
    // --- Procedures: paragraph / flowing text -------------------------
    entry!(
        "tfwrap",
        CapabilityKind::Procedure,
        "Greedy word-wraps a string to a target width under the current font.",
        &[],
        ARTKIT,
        "(str) width tfwrap -> ARRAY",
        LIB
    ),
    entry!(
        "tfflow",
        CapabilityKind::Procedure,
        "General paragraph-flow engine: wraps and draws against a boundsproc ({y -> x0 x1}) until exhausted or out of vertical room.",
        &[],
        ARTKIT,
        "boundsproc y0 ybot leading just (str) tfflow  (leftover) -",
        LIB
    ),
    entry!(
        "tfblock",
        CapabilityKind::Procedure,
        "tfflow's common case: flows a string into a fixed rectangle.",
        &[],
        ARTKIT,
        "x y w h leading just (str) tfblock  (leftover) -",
        LIB
    ),
    entry!(
        "tfcols",
        CapabilityKind::Procedure,
        "Flows a string into multiple columns, carrying leftover from one into the next.",
        &[],
        ARTKIT,
        "x0 y0 colw gap h ncols leading just (str) tfcols  (leftover) -",
        LIB
    ),
    // --- Procedures: layout --------------------------------------------
    entry!(
        "grid",
        CapabilityKind::Procedure,
        "Calls a proc once per cell of a rows x cols grid, with that cell's x y w h.",
        &[],
        ARTKIT,
        "x y w h cols rows {x y w h ...} grid -",
        LIB
    ),
    // --- Procedures: tiling ------------------------------------------------
    entry!(
        "lattice",
        CapabilityKind::Procedure,
        "Walks a 2D point lattice from two basis vectors, calling a proc at each point.",
        &[],
        ARTKIT,
        "x0 y0 v1x v1y v2x v2y n1 n2 {x y ...} lattice -",
        LIB
    ),
    entry!(
        "hex",
        CapabilityKind::Procedure,
        "Appends a pointy-top regular hexagon to the current path.",
        &[],
        ARTKIT,
        "cx cy r hex -",
        LIB
    ),
    entry!(
        "tri",
        CapabilityKind::Procedure,
        "Appends an equilateral triangle to the current path.",
        &[],
        ARTKIT,
        "cx cy s up tri -",
        LIB
    ),
    entry!(
        "hexgrid",
        CapabilityKind::Procedure,
        "The hexagonal tessellation, built on lattice.",
        &[],
        ARTKIT,
        "x y w h cols rows {...} hexgrid -",
        LIB
    ),
    entry!(
        "trigrid",
        CapabilityKind::Procedure,
        "The triangular tessellation, built on lattice.",
        &[],
        ARTKIT,
        "x y w h cols rows {...} trigrid -",
        LIB
    ),
    entry!(
        "truchet",
        CapabilityKind::Procedure,
        "Walks a square grid like grid, rotating each cell a random quarter-turn first — one motif reads as a flowing maze.",
        &[],
        ARTKIT,
        "x y w h cols rows {...} truchet -",
        LIB
    ),
    // --- Procedures: fractals ------------------------------------------
    entry!(
        "edgefractal",
        CapabilityKind::Procedure,
        "Koch-style edge replacement, any turn-delta generator, drawn via rlineto.",
        &[],
        ARTKIT,
        "len angle turns scale depth edgefractal -",
        LIB
    ),
    entry!(
        "edgepoly",
        CapabilityKind::Procedure,
        "Walks edgefractal around a closed polygon (vertices clockwise).",
        &[],
        ARTKIT,
        "[verts] turns scale depth edgepoly -",
        LIB
    ),
    entry!(
        "fgen",
        CapabilityKind::Procedure,
        "Retrieves a named fractal-generator preset (/koch, /quadkoch) from FractalGens.",
        &[],
        ARTKIT,
        "/name fgen -> turns scale",
        LIB
    ),
    entry!(
        "gasket",
        CapabilityKind::Procedure,
        "Sierpinski-style triangle area subdivision, iterative (a stack array, not recursion).",
        &[],
        ARTKIT,
        "x1 y1 x2 y2 x3 y3 depth {proc} gasket -",
        LIB
    ),
    entry!(
        "carpet",
        CapabilityKind::Procedure,
        "Sierpinski-style square area subdivision, iterative.",
        &[],
        ARTKIT,
        "x y w h depth {proc} carpet -",
        LIB
    ),
    // --- Procedures: hyperbolic geometry (Poincare disk) ----------------
    entry!(
        "hpoint",
        CapabilityKind::Procedure,
        "Maps a logical unit-disk point to device space.",
        &[],
        ARTKIT,
        "cx cy r x y hpoint -> dx dy",
        LIB
    ),
    entry!(
        "hpolar",
        CapabilityKind::Procedure,
        "Hyperbolic radius + angle to logical Poincare-disk coordinates.",
        &[],
        ARTKIT,
        "hrad theta hpolar -> x y",
        LIB
    ),
    entry!(
        "horthocircle",
        CapabilityKind::Procedure,
        "The geodesic (orthogonal circle, or a diameter line) through two logical points.",
        &[],
        ARTKIT,
        "x1 y1 x2 y2 horthocircle -> a b r isline",
        LIB
    ),
    entry!(
        "hgeo",
        CapabilityKind::Procedure,
        "Appends one hyperbolic geodesic segment between two logical points to the current path.",
        &[],
        ARTKIT,
        "cx cy r x1 y1 x2 y2 hgeo -",
        LIB
    ),
    entry!(
        "hpoly",
        CapabilityKind::Procedure,
        "Appends a closed polygon of geodesic edges to the current path.",
        &[],
        ARTKIT,
        "cx cy r [v0x v0y v1x v1y ...] hpoly -",
        LIB
    ),
    entry!(
        "hreflect",
        CapabilityKind::Procedure,
        "Reflects a logical point across a geodesic (from horthocircle's result).",
        &[],
        ARTKIT,
        "a b r isline x y hreflect -> x' y'",
        LIB
    ),
    entry!(
        "httile",
        CapabilityKind::Procedure,
        "Breadth-first-reflection generator for regular {p,q} hyperbolic tessellations.",
        &[],
        ARTKIT,
        "cx cy r p q depth {gen proc} httile -",
        LIB
    ),
    // --- Procedures: noise / flow fields ---------------------------------
    entry!(
        "noiseinit",
        CapabilityKind::Procedure,
        "Builds the gradient-noise permutation table for the current srand seed. Call once per srand, before noise2.",
        &[],
        ARTKIT,
        "noiseinit -",
        LIB
    ),
    entry!(
        "noise2",
        CapabilityKind::Procedure,
        "2D coherent (Perlin-style) gradient noise, continuous, 0 at integer lattice points. Needs a prior noiseinit (reads its permutation table).",
        &[],
        ARTKIT,
        "noiseinit  x y noise2 -> n  (~[-0.66, 0.66])",
        LIB
    ),
    entry!(
        "curl2",
        CapabilityKind::Procedure,
        "Turns a scalar field proc into a unit flow vector via its normalized perpendicular gradient.",
        &[],
        ARTKIT,
        "x y eps {x y -> n} curl2 -> dx dy",
        LIB
    ),
    entry!(
        "advect",
        CapabilityKind::Procedure,
        "Traces a particle through a flow field proc as lineto's (stops early on a 0 0 field).",
        &[],
        ARTKIT,
        "x y n stepsize {x y -> dx dy} advect -",
        LIB
    ),
    // --- Procedures: gradients (axial/radial shading fills) --------------
    entry!(
        "gradfn",
        CapabilityKind::Procedure,
        "Builds a PLRM FunctionType 3 (stitching) function from a plain [[r g b] ...] color ramp.",
        &[],
        ARTKIT,
        "[[r g b] ...] gradfn -> functiondict",
        LIB
    ),
    entry!(
        "axialsh",
        CapabilityKind::Procedure,
        "Builds an axial (linear) gradient shading dict.",
        &[],
        ARTKIT,
        "x0 y0 x1 y1 [[r g b] ...] axialsh -> shadingdict",
        LIB
    ),
    entry!(
        "radialsh",
        CapabilityKind::Procedure,
        "Builds a radial gradient shading dict.",
        &[],
        ARTKIT,
        "x0 y0 r0 x1 y1 r1 [[r g b] ...] radialsh -> shadingdict",
        LIB
    ),
    entry!(
        "gradfill",
        CapabilityKind::Procedure,
        "Clips to the current path and shfills a shading dict built by axialsh/radialsh.",
        &[],
        ARTKIT,
        "shadingdict gradfill -",
        LIB
    ),
    // --- Procedures: steampunk style pack (Stage 20) --------------------
    entry!(
        "gear",
        CapabilityKind::Procedure,
        "Toothed wheel with a bored hub (fill or clip it).",
        &[],
        STEAMPUNK,
        "cx cy r n gear -",
        LIB
    ),
    entry!(
        "rivet",
        CapabilityKind::Procedure,
        "A domed rivet: rim plus highlight, drawn with the /spmetal palette.",
        &[],
        STEAMPUNK,
        "cx cy r rivet -",
        LIB
    ),
    entry!(
        "pipe",
        CapabilityKind::Procedure,
        "A flanged pipe run between two points, drawn with the /spmetal palette.",
        &[],
        STEAMPUNK,
        "x1 y1 x2 y2 w pipe -",
        LIB
    ),
    entry!(
        "gauge",
        CapabilityKind::Procedure,
        "A pressure-gauge dial face with a needle at a 0..1 fraction, drawn with the /spmetal palette.",
        &[],
        STEAMPUNK,
        "cx cy r frac gauge -",
        LIB
    ),
    entry!(
        "plateframe",
        CapabilityKind::Procedure,
        "A riveted boilerplate border at a given rivet pitch, drawn with the /spmetal palette.",
        &[],
        STEAMPUNK,
        "x y w h pitch plateframe -",
        LIB
    ),
    entry!(
        "spmetal",
        CapabilityKind::Dial,
        "The palette name steampunk's painted stamps draw with.",
        &[],
        STEAMPUNK,
        "/spmetal /verdigris def   (default /brass)",
        LIB
    ),
    // --- Procedures: psychedelic style pack -------------------------------
    entry!(
        "rainbow",
        CapabilityKind::Procedure,
        "Sets the current color from a hue (0..1, full saturation/value).",
        &[],
        PSYCHEDELIC,
        "t rainbow -",
        LIB
    ),
    entry!(
        "rays",
        CapabilityKind::Procedure,
        "Appends an n-wedge filled sunburst to the current path.",
        &[],
        PSYCHEDELIC,
        "cx cy r n rays -",
        LIB
    ),
    entry!(
        "blob",
        CapabilityKind::Procedure,
        "Appends a wobble-edged circle to the current path — nest and cycle colors for a liquid-ring look.",
        &[],
        PSYCHEDELIC,
        "cx cy r amp lobes blob -",
        LIB
    ),
    entry!(
        "spiral",
        CapabilityKind::Procedure,
        "Appends an Archimedean spiral to the current path.",
        &[],
        PSYCHEDELIC,
        "cx cy rmax turns spiral -",
        LIB
    ),
    entry!(
        "wavy",
        CapabilityKind::Procedure,
        "Appends a sine-displaced line between two points to the current path.",
        &[],
        PSYCHEDELIC,
        "x1 y1 x2 y2 amp waves wavy -",
        LIB
    ),
    entry!(
        "kaleido",
        CapabilityKind::Procedure,
        "Runs a proc n times, in user space, swung i*360/n about a center each time.",
        &[],
        PSYCHEDELIC,
        "cx cy n {proc} kaleido -",
        LIB
    ),
    // --- Procedures: scifi style pack --------------------------------------
    entry!(
        "glowstroke",
        CapabilityKind::Procedure,
        "Strokes the current path three times (halo, mid, bright core) — dark grounds only, consumes the path.",
        &[],
        SCIFI,
        "[r g b] w glowstroke -",
        LIB
    ),
    entry!(
        "starfield",
        CapabilityKind::Procedure,
        "Scatters n stars (a few sparkling) across a rectangle.",
        &[],
        SCIFI,
        "x y w h n starfield -",
        LIB
    ),
    entry!(
        "planet",
        CapabilityKind::Procedure,
        "A banded disc with a terminator shadow and rim, drawn with the /sfworld palette.",
        &[],
        SCIFI,
        "cx cy r planet -",
        LIB
    ),
    entry!(
        "planetring",
        CapabilityKind::Procedure,
        "Two elliptical ring strokes around a planet, at a given tilt.",
        &[],
        SCIFI,
        "cx cy r tilt planetring -",
        LIB
    ),
    entry!(
        "hudcorners",
        CapabilityKind::Procedure,
        "Appends four cockpit-HUD corner brackets to the current path (stroke it).",
        &[],
        SCIFI,
        "x y w h len hudcorners -",
        LIB
    ),
    entry!(
        "reticle",
        CapabilityKind::Procedure,
        "Appends a ring plus tick marks and a gapped crosshair to the current path.",
        &[],
        SCIFI,
        "cx cy r reticle -",
        LIB
    ),
    entry!(
        "hexfield",
        CapabilityKind::Procedure,
        "Appends a honeycomb of side-s hexes filling a rectangle to the current path.",
        &[],
        SCIFI,
        "x y w h s hexfield -",
        LIB
    ),
    entry!(
        "gridfloor",
        CapabilityKind::Procedure,
        "Appends a synthwave perspective floor (rays to a vanishing point, bunching horizontals) to the current path — glowstroke it.",
        &[],
        SCIFI,
        "x y w h nv nh gridfloor -",
        LIB
    ),
    entry!(
        "sfworld",
        CapabilityKind::Dial,
        "The palette name the painted planet draws with.",
        &[],
        SCIFI,
        "/sfworld /hologram def   (default /void)",
        LIB
    ),
    // --- Procedures: toon style pack ----------------------------------
    entry!(
        "celfill",
        CapabilityKind::Procedure,
        "Flat-fills the current path and adds a fat ink outline at a given width — the foundation everything else in this pack is built on.",
        &[],
        TOON,
        "[r g b] w celfill -",
        LIB
    ),
    entry!(
        "bubble",
        CapabilityKind::Procedure,
        "A speech bubble with the tail apex at a given point.",
        &[],
        TOON,
        "x y w h r tx ty bubble -",
        LIB
    ),
    entry!(
        "speedlines",
        CapabilityKind::Procedure,
        "Ink action lines in an angular sweep.",
        &[],
        TOON,
        "cx cy r1 r2 n a0 a1 speedlines -",
        LIB
    ),
    entry!(
        "dotfill",
        CapabilityKind::Procedure,
        "Halftone dots (current color) clipped to the current path — the path survives.",
        &[],
        TOON,
        "pitch r dotfill -",
        LIB
    ),
    entry!(
        "eye",
        CapabilityKind::Procedure,
        "A cartoon eyeball looking toward a given angle.",
        &[],
        TOON,
        "cx cy r ang eye -",
        LIB
    ),
    entry!(
        "burst",
        CapabilityKind::Procedure,
        "Appends a jagged POW-style star to the current path (celfill it).",
        &[],
        TOON,
        "cx cy r1 r2 n burst -",
        LIB
    ),
    entry!(
        "dripbox",
        CapabilityKind::Procedure,
        "Appends a rectangle whose bottom edge melts into n round-tipped drips (celfill it, put a title in it).",
        &[],
        TOON,
        "x y w h n dripbox -",
        LIB
    ),
    entry!(
        "tnink",
        CapabilityKind::Dial,
        "The outline color celfill and friends ink with.",
        &[],
        TOON,
        "/tnink [0.20 0.12 0.30] def   (default near-black)",
        LIB
    ),
    // --- Procedures: lib/handscript.ps (Stage 13) -----------------------
    // Unlike everything above, these take one options dict with named,
    // defaulted keys -- genuinely Param-shaped, same as a page
    // template's content dict -- so parameters is populated for real.
    entry!(
        "hs-write",
        CapabilityKind::Procedure,
        "Draws HandScript text, word-wrapped to the column, top-down from the first baseline. Caller does showpage.",
        &[
            Param {
                name: "Text",
                description: "The note to write (lowercase; the font has no caps)",
                default: None
            },
            Param {
                name: "Size",
                description: "Font size, points",
                default: Some("36")
            },
            Param {
                name: "Width",
                description: "Page width the caller will render at, points",
                default: Some("612")
            },
            Param {
                name: "Height",
                description: "Page height the caller will render at, points",
                default: Some("792")
            },
            Param {
                name: "Margin",
                description: "Page margin, points",
                default: Some("54")
            },
            Param {
                name: "Leading",
                description: "Baseline spacing, multiple of Size",
                default: Some("1.5")
            },
            Param {
                name: "Jitter",
                description: "Ink wobble, glyph units (1000/em)",
                default: Some("16")
            },
            Param {
                name: "Pen",
                description: "Stroke width, glyph units",
                default: Some("22")
            },
            Param {
                name: "Ink",
                description: "RGB 0..1",
                default: Some("[0.13 0.14 0.34]")
            },
            Param {
                name: "Paper",
                description: "/plain or /ruled",
                default: Some("/plain")
            },
            Param {
                name: "Seed",
                description: "rand seed; same seed, same page",
                default: Some("1985")
            },
            Param {
                name: "StartWobble",
                description: "Random per-line indent, points",
                default: Some("10")
            },
        ],
        HANDSCRIPT,
        "opts hs-write -",
        LIB
    ),
    entry!(
        "hs-linecount",
        CapabilityKind::Procedure,
        "Pushes the number of lines the same options dict would wrap to under hs-write, drawing nothing -- run headlessly first to auto-size Height.",
        &[
            Param {
                name: "Text",
                description: "The note to write (lowercase; the font has no caps)",
                default: None
            },
            Param {
                name: "Size",
                description: "Font size, points",
                default: Some("36")
            },
            Param {
                name: "Width",
                description: "Page width the caller will render at, points",
                default: Some("612")
            },
            Param {
                name: "Height",
                description: "Page height the caller will render at, points",
                default: Some("792")
            },
            Param {
                name: "Margin",
                description: "Page margin, points",
                default: Some("54")
            },
            Param {
                name: "Leading",
                description: "Baseline spacing, multiple of Size",
                default: Some("1.5")
            },
            Param {
                name: "Jitter",
                description: "Ink wobble, glyph units (1000/em)",
                default: Some("16")
            },
            Param {
                name: "Pen",
                description: "Stroke width, glyph units",
                default: Some("22")
            },
            Param {
                name: "Ink",
                description: "RGB 0..1",
                default: Some("[0.13 0.14 0.34]")
            },
            Param {
                name: "Paper",
                description: "/plain or /ruled",
                default: Some("/plain")
            },
            Param {
                name: "Seed",
                description: "rand seed; same seed, same page",
                default: Some("1985")
            },
            Param {
                name: "StartWobble",
                description: "Random per-line indent, points",
                default: Some("10")
            },
        ],
        HANDSCRIPT,
        "opts hs-linecount -> n",
        LIB
    ),
    // --- Procedures: lib/hangul.ps (issue #6) ----------------------------
    entry!(
        "hg-write",
        CapabilityKind::Procedure,
        "Draws HangulScript text, word-wrapped to the column, top-down from the first baseline. Only modern Hangul syllables render; any ASCII/space/punctuation mixed in advances the cursor but draws nothing (see HangulScript's own description).",
        &[
            Param {
                name: "Text",
                description: "UTF-8 text; only Hangul syllables (U+AC00-U+D7A3) render -- ASCII/spaces/punctuation advance but stay invisible",
                default: None
            },
            Param {
                name: "Size",
                description: "Font size, points",
                default: Some("48")
            },
            Param {
                name: "Width",
                description: "Page width the caller will render at, points",
                default: Some("612")
            },
            Param {
                name: "Height",
                description: "Page height the caller will render at, points",
                default: Some("792")
            },
            Param {
                name: "Margin",
                description: "Page margin, points",
                default: Some("54")
            },
            Param {
                name: "Leading",
                description: "Baseline spacing, multiple of Size",
                default: Some("1.5")
            },
            Param {
                name: "Jitter",
                description: "Ink wobble, glyph units (1000/em)",
                default: Some("12")
            },
            Param {
                name: "Pen",
                description: "Stroke width, glyph units",
                default: Some("24")
            },
            Param {
                name: "Ink",
                description: "RGB 0..1",
                default: Some("[0.1 0.1 0.1]")
            },
            Param {
                name: "Seed",
                description: "rand seed; same seed, same page",
                default: Some("1985")
            },
        ],
        HANGUL,
        "opts hg-write -",
        LIB
    ),
    entry!(
        "hg-linecount",
        CapabilityKind::Procedure,
        "Pushes the line count the same options dict would wrap to under hg-write, drawing nothing.",
        &[
            Param {
                name: "Text",
                description: "UTF-8 text; only Hangul syllables (U+AC00-U+D7A3) render under hg-write -- ASCII/spaces/punctuation advance but stay invisible",
                default: None
            },
            Param {
                name: "Size",
                description: "Font size, points",
                default: Some("48")
            },
            Param {
                name: "Width",
                description: "Page width the caller will render at, points",
                default: Some("612")
            },
            Param {
                name: "Height",
                description: "Page height the caller will render at, points",
                default: Some("792")
            },
            Param {
                name: "Margin",
                description: "Page margin, points",
                default: Some("54")
            },
            Param {
                name: "Leading",
                description: "Baseline spacing, multiple of Size",
                default: Some("1.5")
            },
            Param {
                name: "Jitter",
                description: "Ink wobble, glyph units (1000/em)",
                default: Some("12")
            },
            Param {
                name: "Pen",
                description: "Stroke width, glyph units",
                default: Some("24")
            },
            Param {
                name: "Ink",
                description: "RGB 0..1",
                default: Some("[0.1 0.1 0.1]")
            },
            Param {
                name: "Seed",
                description: "rand seed; same seed, same page",
                default: Some("1985")
            },
        ],
        HANGUL,
        "opts hg-linecount -> n",
        LIB
    ),
];

/// Builds the full catalog: [`ENTRIES`] plus every font `findfont` can
/// currently reach (from [`font::catalog_entries`], so this can't
/// disagree with `--fonts`/`resolve` about what's installed).
pub fn catalog() -> Vec<Capability> {
    let mut caps: Vec<Capability> = ENTRIES.iter().map(Entry::to_capability).collect();
    for f in font::catalog_entries() {
        let (description, availability) = match f.origin {
            font::FontOrigin::Builtin => (
                "Bundled base-13 face; resolves identically everywhere, wasm included.".to_string(),
                "builtin",
            ),
            font::FontOrigin::Catalog => (
                "Font-catalog face, loaded from fonts/catalog/ at findfont time.".to_string(),
                "catalog (desktop/bundle only; absent on wasm or an incomplete install)",
            ),
            font::FontOrigin::Alias => (
                format!(
                    "findfont alias for the catalog face {}.",
                    f.alias_target.as_deref().unwrap_or("?")
                ),
                "catalog (desktop/bundle only; absent on wasm or an incomplete install)",
            ),
        };
        caps.push(Capability {
            name: f.name,
            kind: CapabilityKind::Font,
            description,
            parameters: Vec::new(),
            source: "fonts/".to_string(),
            load: String::new(),
            example: "/Name findfont 24 scalefont setfont".to_string(),
            availability: availability.to_string(),
        });
    }
    caps
}

/// A short, deterministic fingerprint of exactly which capabilities
/// are available (name/kind/availability triples — not their prose,
/// which can change without anything actually becoming reachable or
/// unreachable). `pscat_version` alone under-signals drift for the one
/// section built from the filesystem: a font catalog can gain or lose
/// files (a different `PSCAT_ROOT`, or an install updated in place)
/// without the binary's own version changing at all (a cross-model
/// review finding, PR #74). Callers should treat `pscat_version` as a
/// change **or** `catalog_signature` as a change as the re-fetch
/// signal — either one moving means something did.
fn catalog_signature(caps: &[Capability]) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for c in caps {
        c.name.hash(&mut hasher);
        c.kind.as_str().hash(&mut hasher);
        c.availability.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}

/// The full catalog as one JSON payload: `pscat_version` and
/// `catalog_signature` (drift detection — see [`catalog_signature`])
/// plus the deterministically name-then-kind-sorted capability list
/// (so two dumps of the same release/install diff cleanly regardless
/// of hashmap/readdir iteration order upstream).
pub fn payload_json() -> Value {
    let mut caps = catalog();
    caps.sort_by(|a, b| (&a.name, a.kind.as_str()).cmp(&(&b.name, b.kind.as_str())));
    json!({
        "pscat_version": env!("CARGO_PKG_VERSION"),
        "catalog_signature": catalog_signature(&caps),
        "capabilities": caps.iter().map(Capability::to_json).collect::<Vec<_>>(),
    })
}
