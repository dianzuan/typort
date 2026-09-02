use super::{
    EquationElem, FromStr, HeadingElem, Lang, Paragraph, ParagraphStyle, Region, Run, Smart,
    SmartQuoteElem, SmartQuoter, SmartQuotes, StyleChain, Tag, WalkCtx, content_at_location,
    inline,
};

/// Handle a `HeadingElem` tag: query the introspector for the full Content,
/// extract level + body runs, emit a heading paragraph, and return its level.
pub(super) fn handle_heading(tag: &Tag, ctx: &mut WalkCtx) -> Option<usize> {
    let content = content_at_location(ctx.html_doc, tag.location())?;
    let heading = content.to_packed::<HeadingElem>()?;

    let level = heading.resolve_level(StyleChain::default()).get();
    #[allow(clippy::cast_possible_truncation)]
    let level_u8 = level.min(255) as u8;

    let mut para = Paragraph::new();
    para.style = Some(ParagraphStyle::Heading(level_u8));

    // Insert bookmark if heading has a label
    if let Some(label) = content.label() {
        ctx.add_bookmark(&mut para, label.resolve().to_string());
    }

    // Prepend the synthesized heading number ("一、", "(三)", "1.1", …). Typst's
    // numbering show rule renders it but it lives in this introspector field, not
    // in `heading.body`, so the AST walk below would otherwise drop it. Language-
    // and scheme-agnostic: whatever Typst computed is emitted verbatim.
    if let Some(numbers) = heading.numbers.as_ref()
        && !numbers.is_empty()
    {
        para.push_run(Run::new(format!("{numbers} ")));
    }

    // Walk heading body — text runs, inline math, and smart quotes. Quotes live in
    // the AST as unresolved SmartQuoteElem (open/close depends on context), so we
    // resolve them with Typst's own SmartQuoter, using the document's language.
    let (lang, region) = smart_quote_lang(ctx.doc);
    let quote_dict = Smart::Auto;
    let quotes = SmartQuotes::get(&quote_dict, lang, region, false);
    let mut quote_state = SmartQuoter::new();
    // The body starts after the (optional) number prefix + space, so the first
    // quote should open: treat the preceding char as absent (the quote_state reads that
    // as a space).
    let mut prev_char: Option<char> = None;
    extract_heading_content(
        &heading.body,
        &mut para,
        &quotes,
        &mut quote_state,
        &mut prev_char,
    );
    ctx.doc.add_paragraph(para);
    Some(level)
}

/// Derive the smart-quote language/region from the document's declared language.
pub(super) fn smart_quote_lang(doc: &typort_ooxml::document::Document) -> (Lang, Option<Region>) {
    let mut parts = doc.style.lang_latin.split('-');
    let lang = parts
        .next()
        .and_then(|l| Lang::from_str(l).ok())
        .unwrap_or(Lang::ENGLISH);
    let region = parts.next().and_then(|r| Region::from_str(r).ok());
    (lang, region)
}

/// Walk a heading's body content, extracting text runs, inline math, and smart
/// quotes (resolved via `quote_state`/`quotes` with `prev_char` as the preceding char).
pub(super) fn extract_heading_content(
    content: &typst::foundations::Content,
    para: &mut Paragraph,
    quotes: &SmartQuotes<'_>,
    quote_state: &mut SmartQuoter,
    prev_char: &mut Option<char>,
) {
    use typst_library::foundations::SequenceElem;

    if let Some(seq) = content.to_packed::<SequenceElem>() {
        for child in &seq.children {
            extract_heading_content(child, para, quotes, quote_state, prev_char);
        }
    } else if content.to_packed::<EquationElem>().is_some() {
        let omml = typort_math::equation_to_omml(content);
        para.add_math(omml);
        *prev_char = Some('\u{FFFC}'); // object replacement: an equation acts as an object
    } else if let Some(sq) = content.to_packed::<SmartQuoteElem>() {
        let double = *sq.double.as_option().as_ref().unwrap_or(&true);
        let quote = quote_state.quote(*prev_char, quotes, double);
        para.push_run(Run::new(quote));
        *prev_char = quote.chars().next_back();
    } else {
        for run in inline::extract_runs(content) {
            if let Some(c) = run.text.chars().next_back() {
                *prev_char = Some(c);
            }
            para.push_run(run);
        }
    }
}
