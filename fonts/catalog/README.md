# fonts/catalog — the runtime font catalog

A wide shelf of libre faces that `findfont` loads **from disk on first
use** — nothing here is compiled into the binary (the twelve Liberation
faces in `fonts/` stay the only bundled ones, so the binary and the
wasm build stay lean). Drop a `.ttf`/`.otf`/`.ttc` into any subdirectory
here and `/<FileStem> findfont` finds it; `/<Family> findfont` falls
back to `<Family>-Regular`. `pscat --fonts` lists everything reachable.
(`.ttc` collections always load face index 0 — untested against a real
collection, since nothing bundled today ships as one.)

The catalog root is found via `$PSCAT_ROOT`, the executable's
location, the build-time checkout path, or the working directory — see
`catalog_root()` in `src/font.rs`. Every family directory carries its
own license text alongside its font files. Licenses here are the SIL
Open Font License 1.1 (OFL), Apache 2.0, the GUST Font License (GFL),
and AGPL-3.0-with-font-exception — all freely redistributable; none
restrict rendering or embedding in documents.

## The rest of the PostScript standard 35

With these, every one of the classic LaserWriter 35 names resolves to
a metric-compatible libre face (the alias table lives in
`src/font.rs`):

| Directory | Standard names covered | Face | License |
|---|---|---|---|
| `TeXGyre-pagella` | Palatino-Roman/-Bold/-Italic/-BoldItalic | TeX Gyre Pagella (GUST) | GFL |
| `TeXGyre-bonum` | Bookman-Light/-Demi/-LightItalic/-DemiItalic | TeX Gyre Bonum (GUST) | GFL |
| `TeXGyre-schola` | NewCenturySchlbk-Roman/-Bold/-Italic/-BoldItalic | TeX Gyre Schola (GUST) | GFL |
| `TeXGyre-adventor` | AvantGarde-Book/-Demi/-BookOblique/-DemiOblique | TeX Gyre Adventor (GUST) | GFL |
| `TeXGyre-chorus` | ZapfChancery-MediumItalic | TeX Gyre Chorus (GUST) | GFL |
| `TeXGyre-heroscn` | Helvetica-Narrow (+Bold/Oblique/BoldOblique) | TeX Gyre Heros Cn (GUST) | GFL |
| `URW-symbols` | Symbol, ZapfDingbats | URW Standard Symbols PS, D050000L | AGPL-3.0 + font exception |

The URW faces ship with their own PLRM encodings (SymbolEncoding /
DingbatsEncoding, dumped from Ghostscript — `src/encodings.rs`).

## Display, text, and mood faces (one style each)

All from the Google Fonts collection; each directory carries the
family's `OFL.txt` or Apache `LICENSE.txt`.

| Face | Genre / mood | License |
|---|---|---|
| EBGaramond | old-style garalde serif (Garamond; aliased `/Garamond`) | OFL |
| LibreBaskerville | transitional serif (aliased `/Baskerville`) | OFL |
| PlayfairDisplay | didone display serif | OFL |
| Lora | contemporary text serif | OFL |
| Cinzel | classical Roman capitals (Trajan spirit) | OFL |
| Poppins | geometric sans | OFL |
| Jost | Futura-spirited geometric sans | OFL |
| Oswald | condensed gothic sans | OFL |
| AtkinsonHyperlegible | high-legibility humanist sans | OFL |
| ZillaSlab | contemporary slab serif | OFL |
| AlfaSlabOne | fat-face slab display | OFL |
| JetBrainsMono | modern coding monospace | OFL |
| GreatVibes | formal copperplate script | OFL |
| Pacifico | 1950s brush script | OFL |
| Caveat | casual handwriting | OFL |
| HomemadeApple | flowing pen handwriting | Apache 2.0 |
| BebasNeue | tall condensed poster caps | OFL |
| AbrilFatface | heavy didone advertising face | OFL |
| Monoton | inline neon-sign display | OFL |
| Limelight | art-deco marquee | OFL |
| Bungee | chromatic urban signage | OFL |
| UnifrakturMaguntia | blackletter fraktur | OFL |
| PirataOne | condensed blackletter | OFL |
| Rye | western circus/wanted-poster | OFL |
| Creepster | horror-movie lettering | OFL |
| PressStart2P | 8-bit arcade pixels | OFL |
| VT323 | glowing-terminal mono | OFL |
| SpecialElite | worn typewriter | Apache 2.0 |
| Orbitron | sci-fi geometric display | OFL |
| Audiowide | streamlined techno display | OFL |
| StardosStencil | stencil serif | OFL |
| Bangers | comic-book shout | OFL |
| ComicNeue | the casual comic sans, done right | OFL |
| MedievalSharp | carved medieval lettering | OFL |
| PermanentMarker | felt-tip marker | Apache 2.0 |

Sources: TeX Gyre from CTAN (`fonts/tex-gyre`), URW faces from
`github.com/ArtifexSoftware/urw-base35-fonts`, everything else from
`github.com/google/fonts` (`ofl/` and `apache/` trees).

## Korean, Japanese, and Thai (Unicode-mode)

Hangul and kanji don't fit in the 256-slot Encoding model every other
face here uses, so these three get a documented deviation instead:
`show` decodes their strings as UTF-8 and maps codepoints straight to
glyphs (see FONTS.md's "Unicode-mode catalog faces" addendum). Literal
Korean/Japanese/Thai text in a `(...)` string just works — no custom
Encoding array needed. Both are variable fonts pinned to their Regular
weight (`wght` 400) at load time rather than the file's own default
instance, which is Thin for these two.

| Face | Script | License |
|---|---|---|
| NotoSansKR | Korean (Hangul + Latin) | OFL |
| NotoSansJP | Japanese (kana + kanji + Latin) | OFL |
| NotoSansThai | Thai (+ Latin) | OFL |

Source: `github.com/google/fonts` (`ofl/notosanskr`, `ofl/notosansjp`,
`ofl/notosansthai`).

A second Korean face rides the same Unicode-mode mechanism, for a
handwritten/brush look rather than plain sans:

| Face | Script | License |
|---|---|---|
| NanumBrushScript | Korean, brush calligraphy | OFL |

Source: `github.com/google/fonts` (`ofl/nanumbrushscript`). Static (not
variable), so no default-instance pinning needed.

For the *programmatic* faces — glyphs that are PostScript procedures,
not outlines — see `lib/fonts/`.
