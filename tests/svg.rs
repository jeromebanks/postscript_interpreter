//! SVG export — Stage 9 task 4. Structural assertions on the emitted
//! documents; visual fidelity was verified by rasterizing the SVGs in
//! a browser and comparing with the canvas (postcard.ps round-trips).

use pscat::Interp;

fn svg_pages(src: &str) -> Vec<String> {
    let mut it = Interp::with_page(100, 100).expect("test page");
    it.gfx_mut().enable_svg();
    it.run_str(src)
        .unwrap_or_else(|e| panic!("run failed: {}", it.error_report(&e)));
    it.gfx().svg_pages().expect("recording enabled")
}

#[test]
fn fill_becomes_a_path_element() {
    let pages = svg_pages("newpath 10 10 moveto 90 10 lineto 50 90 lineto closepath fill");
    assert_eq!(pages.len(), 1);
    let svg = &pages[0];
    // Device space flips y: user (10,10) is device (10,90).
    assert!(svg.contains("<path d=\"M10 90L90 90L50 10Z\""), "{svg}");
    assert!(svg.contains("fill=\"#000000\""));
    assert!(svg.starts_with("<svg xmlns"));
}

#[test]
fn stroke_carries_width_and_dashes() {
    let pages = svg_pages(
        "1 0 0 setrgbcolor 4 setlinewidth [6 2] 1 setdash
         newpath 10 50 moveto 90 50 lineto stroke",
    );
    let svg = &pages[0];
    assert!(svg.contains("stroke=\"#ff0000\""), "{svg}");
    assert!(svg.contains("stroke-width=\"4\""));
    assert!(svg.contains("stroke-dasharray=\"6 2\""));
    assert!(svg.contains("stroke-dashoffset=\"1\""));
    assert!(svg.contains("fill=\"none\""));
}

#[test]
fn clip_produces_clippath_defs_and_groups() {
    let pages = svg_pages(
        "newpath 20 20 moveto 80 20 lineto 80 80 lineto 20 80 lineto closepath clip newpath
         newpath 0 0 moveto 100 0 lineto 100 100 lineto 0 100 lineto closepath fill",
    );
    let svg = &pages[0];
    assert!(svg.contains("<clipPath id=\"c0\""), "{svg}");
    assert!(svg.contains("<g clip-path=\"url(#c0)\">"));
}

#[test]
fn nested_clips_nest_groups() {
    let pages = svg_pages(
        "newpath 10 10 moveto 90 10 lineto 90 90 lineto 10 90 lineto closepath clip newpath
         newpath 30 30 moveto 70 30 lineto 70 70 lineto 30 70 lineto closepath clip newpath
         newpath 0 0 moveto 100 0 lineto 100 100 lineto 0 100 lineto closepath fill",
    );
    let svg = &pages[0];
    assert!(
        svg.contains("clip-path=\"url(#c0)\"") && svg.contains("clip-path=\"url(#c1)\""),
        "both clip links present: {svg}"
    );
}

#[test]
fn images_embed_base64_png() {
    let pages = svg_pages(
        "10 10 translate 80 80 scale
         2 2 8 [2 0 0 -2 0 2] {<00ff ff00>} image",
    );
    let svg = &pages[0];
    assert!(
        svg.contains("data:image/png;base64,"),
        "embedded png: {svg}"
    );
    assert!(svg.contains("<image width=\"2\" height=\"2\""));
    assert!(svg.contains("transform=\"matrix("));
}

#[test]
fn glyphs_arrive_as_outline_paths() {
    let pages = svg_pages("/Helvetica findfont 30 scalefont setfont 10 40 moveto (I) show");
    let svg = &pages[0];
    // At least one filled path beyond the background rect.
    assert!(svg.matches("<path").count() >= 1, "{svg}");
}

#[test]
fn pages_split_like_the_raster() {
    let pages = svg_pages(
        "newpath 10 10 moveto 90 10 lineto 50 90 lineto closepath fill showpage
         newpath 20 20 moveto 80 20 lineto 80 80 lineto 20 80 lineto closepath fill showpage",
    );
    assert_eq!(pages.len(), 2);
    assert!(pages[0].contains("L50 10Z"), "page 1 has the triangle");
    assert!(
        !pages[1].contains("L50 10Z"),
        "page 2 erased the triangle (lazy erase mirrored)"
    );
}

#[test]
fn single_page_program_yields_one_document() {
    let pages = svg_pages("newpath 10 10 moveto 90 10 lineto 50 90 lineto closepath fill showpage");
    assert_eq!(pages.len(), 1, "trailing blank canvas is not a page");
}

#[test]
fn shfill_axial_becomes_a_linear_gradient() {
    let pages = svg_pages(
        "<< /ShadingType 2 /ColorSpace /DeviceRGB /Coords [0 0 100 0] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [0 0 0] /C1 [1 1 1] /N 1 >> >> shfill",
    );
    let svg = &pages[0];
    assert!(
        svg.contains(
            "<linearGradient id=\"g0\" gradientUnits=\"userSpaceOnUse\" \
                       x1=\"0\" y1=\"0\" x2=\"100\" y2=\"0\""
        ),
        "{svg}"
    );
    assert!(svg.contains("spreadMethod=\"pad\""), "{svg}");
    assert!(svg.contains("<stop offset=\"0\""), "{svg}");
    assert!(svg.contains("<stop offset=\"1\""), "{svg}");
    assert!(svg.contains("fill=\"url(#g0)\""), "{svg}");
}

#[test]
fn shfill_radial_burst_omits_fr_but_two_circle_includes_it() {
    // r0 = 0 (the common burst case): plain SVG 1.1 markup, no `fr`.
    let pages = svg_pages(
        "<< /ShadingType 3 /ColorSpace /DeviceRGB /Coords [50 50 0 50 50 40] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> >> shfill",
    );
    let svg = &pages[0];
    assert!(
        svg.contains(
            "<radialGradient id=\"g0\" gradientUnits=\"userSpaceOnUse\" \
                       cx=\"50\" cy=\"50\" r=\"40\""
        ),
        "{svg}"
    );
    assert!(!svg.contains("fr=\""), "r0=0 should not emit fr: {svg}");

    // r0 > 0 (a genuine two-circle gradient): SVG2's fx/fy/fr.
    let pages = svg_pages(
        "<< /ShadingType 3 /ColorSpace /DeviceRGB /Coords [30 30 10 60 60 40] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> >> shfill",
    );
    let svg = &pages[0];
    assert!(
        svg.contains("cx=\"60\" cy=\"60\" r=\"40\" fx=\"30\" fy=\"30\" fr=\"10\""),
        "{svg}"
    );
}

#[test]
fn sh_shrinking_radial_swaps_circles_and_reverses_stops() {
    // PostScript allows r0 > r1 (a burst that shrinks toward its
    // center); SVG requires fr <= r, so the bigger circle (here the
    // *start*, r0=40) must become cx/cy/r (the outer circle) and the
    // smaller (r1=10, the *end*) becomes fx/fy/fr, with stop offsets
    // reversed to compensate (SVG's offset 0 is always the focal
    // circle, offset 1 the outer one).
    let pages = svg_pages(
        "<< /ShadingType 3 /ColorSpace /DeviceRGB /Coords [50 50 40 50 50 10] \
         /Function << /FunctionType 2 /Domain [0 1] /C0 [1 0 0] /C1 [0 0 1] /N 1 >> >> shfill",
    );
    let svg = &pages[0];
    assert!(
        svg.contains("cx=\"50\" cy=\"50\" r=\"40\" fx=\"50\" fy=\"50\" fr=\"10\""),
        "{svg}"
    );
    // C0 (red, PostScript t=0/r0=40, the *outer* circle after the
    // swap) must land at SVG offset 1; C1 (blue, t=1/r1=10, the focal
    // circle) at offset 0.
    assert!(
        svg.contains("<stop offset=\"0\" stop-color=\"#0000ff\"/>"),
        "{svg}"
    );
    assert!(
        svg.contains("<stop offset=\"1\" stop-color=\"#ff0000\"/>"),
        "{svg}"
    );
}
