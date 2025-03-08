use pulldown_cmark::{CodeBlockKind, Event, Tag, TagEnd};
use std::iter::Iterator;
use std::sync::OnceLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::html::{
    append_highlighted_html_for_styled_line, start_highlighted_html_snippet, IncludeBackground,
};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

pub struct SyntaxHighligher {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl SyntaxHighligher {
    pub fn load() -> Self {
        let mut theme_set = ThemeSet::new(); // empty

        let github_dark: Theme = ThemeSet::load_from_reader(&mut std::io::Cursor::new(
            include_bytes!("./vendor/GitHub_Dark.tmTheme"),
        ))
        .unwrap();
        let github_light: Theme = ThemeSet::load_from_reader(&mut std::io::Cursor::new(
            include_bytes!("./vendor/GitHub_Light.tmTheme"),
        ))
        .unwrap();

        theme_set
            .themes
            .insert("github-dark".to_string(), github_dark);
        theme_set
            .themes
            .insert("github-light".to_string(), github_light);

        //for theme in theme_set.themes.iter_mut() {
        //    theme.1.settings.background = None;
        //}

        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set,
        }
    }

    pub fn highlight(&self, code: &str, lang: Option<&str>) -> String {
        //let syntax = lang
        //    .and_then(|l| self.syntax_set.find_syntax_by_token(l))
        //    .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());
        //
        //let mut output = String::with_capacity(64);
        //output.push_str("<pre><code>");
        //
        //let mut html_generator = ClassedHTMLGenerator::new_with_class_style(
        //    syntax, &self.syntax_set, ClassStyle::Spaced);
        //
        //for line in LinesWithEndings::from(code) {
        //    html_generator.parse_html_for_line_which_includes_newline(line).unwrap();
        //}
        //let inner = html_generator.finalize();
        //print!("{}", inner);
        //output.push_str(&inner);
        //output.push_str("</code></pre>");
        //output

        // TODO: we want to use classed html and generate CSS from the theme so everything below is
        // supposed to be removed.
        // See: https://docs.rs/syntect/latest/syntect/html/fn.css_for_theme_with_class_style.html

        let theme = &self.theme_set.themes["github-dark"];

        let syntax = lang
            .and_then(|l| self.syntax_set.find_syntax_by_token(l))
            .unwrap_or_else(|| self.syntax_set.find_syntax_plain_text());

        let mut highlighter = HighlightLines::new(syntax, theme);

        let (mut output, bg) = start_highlighted_html_snippet(theme);

        output.push_str("<code>");

        for line in LinesWithEndings::from(code) {
            let regions = highlighter.highlight_line(line, &self.syntax_set).unwrap();
            append_highlighted_html_for_styled_line(
                &regions[..],
                IncludeBackground::IfDifferent(bg),
                &mut output,
            )
            .unwrap();
        }

        output.push_str("</code></pre>\n");
        output
    }
}

fn syntax() -> &'static SyntaxHighligher {
    static SYNTAX: OnceLock<SyntaxHighligher> = OnceLock::new();
    let syntax = SYNTAX.get_or_init(SyntaxHighligher::load);
    syntax
}

pub(crate) fn map_highlighted_codeblocks<'a>(
    parser: impl Iterator<Item = Event<'a>>,
) -> impl Iterator<Item = Event<'a>> {
    let syntax = syntax();
    let mut in_code_block = false;
    let mut lang = None;

    let parser = parser.map(move |event| match event {
        Event::Start(Tag::CodeBlock(kind)) => {
            in_code_block = true;
            let tag = match kind {
                CodeBlockKind::Indented => "",
                CodeBlockKind::Fenced(ref tag) => tag.as_ref(),
            };
            lang = tag.split(' ').map(|s| s.to_owned()).next();
            Event::Text(pulldown_cmark::CowStr::Borrowed(""))
        }

        Event::End(TagEnd::CodeBlock) => Event::Text(pulldown_cmark::CowStr::Borrowed("")),
        Event::Text(code) if in_code_block => {
            let html = syntax.highlight(code.as_ref(), lang.as_deref());
            in_code_block = false;
            lang = None;
            Event::Html(pulldown_cmark::CowStr::Boxed(html.into_boxed_str()))
        }
        _ => event,
    });
    parser
}
