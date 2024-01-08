use winnow::{
    combinator::{alt, cut_err, fold_repeat, preceded, terminated},
    error::{ContextError, ParseError},
    stream::{AsChar, ContainsToken, Range},
    token::{none_of, one_of, tag, take_till, take_until0, take_while},
    PResult, Parser,
};

/// ```text
///         v----v inner span
/// :global(.class)
/// ^-------------^ outer span
/// ```
#[derive(Debug, PartialEq)]
pub struct Global<'s> {
    pub inner: &'s str,
    pub outer: &'s str,
}

#[derive(Debug, PartialEq)]
pub enum CssFragment<'s> {
    Class(&'s str),
    Global(Global<'s>),
}

pub fn parse_css(input: &str) -> Result<Vec<CssFragment>, ParseError<&str, ContextError>> {
    style_rule_list.parse(input)
}

pub fn recognize_repeat<'s, O>(
    range: impl Into<Range>,
    f: impl Parser<&'s str, O, ContextError>,
) -> impl Parser<&'s str, &'s str, ContextError> {
    fold_repeat(range, f, || (), |_, _| ()).recognize()
}

fn ws<'s>(input: &mut &'s str) -> PResult<&'s str> {
    recognize_repeat(
        0..,
        alt((
            line_comment,
            block_comment,
            take_while(1.., (AsChar::is_space, '\n', '\r')),
        )),
    )
    .parse_next(input)
}

fn line_comment<'s>(input: &mut &'s str) -> PResult<&'s str> {
    ("//", take_while(0.., |c| c != '\n'))
        .recognize()
        .parse_next(input)
}

fn block_comment<'s>(input: &mut &'s str) -> PResult<&'s str> {
    ("/*", cut_err(terminated(take_until0("*/"), "*/")))
        .recognize()
        .parse_next(input)
}

fn identifier<'s>(input: &mut &'s str) -> PResult<&'s str> {
    (
        one_of(('_', '-', AsChar::is_alpha)),
        take_while(0.., ('_', '-', AsChar::is_alphanum)),
    )
        .recognize()
        .parse_next(input)
}

fn class<'s>(input: &mut &'s str) -> PResult<&'s str> {
    preceded('.', identifier).parse_next(input)
}

fn global<'s>(input: &mut &'s str) -> PResult<Global<'s>> {
    let (inner, outer) = preceded(
        ":global(",
        cut_err(terminated(
            stuff_till(0.., (')', '(', '{')), // inner
            ')',
        )),
    )
    .with_recognized() // outer
    .parse_next(input)?;
    Ok(Global { inner, outer })
}

fn string_dq<'s>(input: &mut &'s str) -> PResult<&'s str> {
    let str_char = alt((none_of(['"']).map(|_| ()), tag("\\\"").map(|_| ())));
    let str_chars = recognize_repeat(0.., str_char);

    preceded('"', cut_err(terminated(str_chars, '"'))).parse_next(input)
}

fn string_sq<'s>(input: &mut &'s str) -> PResult<&'s str> {
    let str_char = alt((none_of(['\'']).map(|_| ()), tag("\\'").map(|_| ())));
    let str_chars = recognize_repeat(0.., str_char);

    preceded('\'', cut_err(terminated(str_chars, '\''))).parse_next(input)
}

fn string<'s>(input: &mut &'s str) -> PResult<&'s str> {
    alt((string_dq, string_sq)).parse_next(input)
}

/// Behaves like take_till except it finds and parses strings and
/// comments (allowing those to contain the end condition characters).
pub fn stuff_till<'s>(
    range: impl Into<Range>,
    list: impl ContainsToken<char>,
) -> impl Parser<&'s str, &'s str, ContextError> {
    fold_repeat(
        range,
        alt((
            string.map(|_| ()),
            block_comment.map(|_| ()),
            line_comment.map(|_| ()),
            '/'.map(|_| ()),
            take_till(1.., ('\'', '"', '/', list)).map(|_| ()),
        )),
        || (),
        |_, _| (),
    )
    .recognize()
}

fn selector<'s>(input: &mut &'s str) -> PResult<Vec<CssFragment<'s>>> {
    fold_repeat(
        1..,
        alt((
            class.map(|c| Some(CssFragment::Class(c))),
            global.map(|g| Some(CssFragment::Global(g))),
            ':'.map(|_| None),
            stuff_till(1.., ('.', ';', '{', '}', ':')).map(|_| None),
        )),
        Vec::new,
        |mut acc, item| {
            if let Some(item) = item {
                acc.push(item);
            }
            acc
        },
    )
    .parse_next(input)
}

fn declaration<'s>(input: &mut &'s str) -> PResult<&'s str> {
    (
        identifier,
        ws,
        ':',
        terminated(stuff_till(1.., (';', '{', '}')), ';'),
    )
        .recognize()
        .parse_next(input)
}

fn style_rule_block<'s>(input: &mut &'s str) -> PResult<Vec<CssFragment<'s>>> {
    let content = alt((
        declaration.map(|_| None), //
        at_rule.map(Some),
        style_rule.map(Some),
    ));
    let contents = fold_repeat(0.., (ws, content), Vec::new, |mut acc, item| {
        if let Some(mut item) = item.1 {
            acc.append(&mut item);
        }
        acc
    });

    preceded('{', cut_err(terminated(contents, (ws, '}')))).parse_next(input)
}

fn style_rule<'s>(input: &mut &'s str) -> PResult<Vec<CssFragment<'s>>> {
    let (mut classes, mut nested_classes) = (selector, style_rule_block).parse_next(input)?;
    classes.append(&mut nested_classes);
    Ok(classes)
}

fn at_rule<'s>(input: &mut &'s str) -> PResult<Vec<CssFragment<'s>>> {
    let (identifier, char) = preceded(
        '@',
        cut_err((
            terminated(identifier, stuff_till(0.., ('{', '}', ';'))),
            one_of(('{', ';')),
        )),
    )
    .parse_next(input)?;

    if char == ';' {
        return Ok(vec![]);
    }

    if identifier == "media" {
        cut_err(terminated(style_rule_list, '}')).parse_next(input)
    } else {
        cut_err(terminated(unknown_block_contents, '}')).parse_next(input)?;
        Ok(vec![])
    }
}

fn unknown_block_contents<'s>(input: &mut &'s str) -> PResult<&'s str> {
    recognize_repeat(
        0..,
        alt((
            stuff_till(1.., ('{', '}')).map(|_| ()),
            ('{', cut_err((unknown_block_contents, '}'))).map(|_| ()),
        )),
    )
    .parse_next(input)
}

fn style_rule_list<'s>(input: &mut &'s str) -> PResult<Vec<CssFragment<'s>>> {
    terminated(
        fold_repeat(0.., style_rule, Vec::new, |mut acc, mut item| {
            acc.append(&mut item);
            acc
        }),
        ws,
    )
    .parse_next(input)
}

#[test]
fn test_class() {
    let mut input = "._x1a2b Hello";

    let r = class.parse_next(&mut input);
    assert_eq!(r, Ok("_x1a2b"));
}

#[test]
fn test_selector() {
    let mut input = ".foo.bar [value=\"fa.sdasd\"] /* .banana */ // .apple \n \t .cry {";

    let r = selector.parse_next(&mut input);
    assert_eq!(
        r,
        Ok(vec![
            CssFragment::Class("foo"),
            CssFragment::Class("bar"),
            CssFragment::Class("cry")
        ])
    );

    let mut input = "{";

    let r = selector.recognize().parse_next(&mut input);
    assert!(r.is_err());
}

#[test]
fn test_declaration() {
    let mut input = "background-color \t : red;";

    let r = declaration.parse_next(&mut input);
    assert_eq!(r, Ok("background-color \t : red;"));

    let r = declaration.parse_next(&mut input);
    assert!(r.is_err());
}

#[test]
fn test_style_rule() {
    let mut input = ".foo.bar {
        background-color: red;
        .baz {
            color: blue;
        }
        @some-at-rule blah blah;
        @media blah .blah {
            .moo {
                color: red;
            }
        }
    }END";

    let r = style_rule.parse_next(&mut input);
    assert_eq!(
        r,
        Ok(vec![
            CssFragment::Class("foo"),
            CssFragment::Class("bar"),
            CssFragment::Class("baz"),
            CssFragment::Class("moo")
        ])
    );

    assert_eq!(input, "END");
}

#[test]
fn test_style_rule_list() {
    let mut input = "
        .foo.bar :global(.global) {
            background-color \t\r\n : red;
            color: blue;
        }
        
        .baz.moo {
            color: red;
            .rad {
                color: red;
            }
        }
    ";

    let r = style_rule_list.parse_next(&mut input);
    assert_eq!(
        r,
        Ok(vec![
            CssFragment::Class("foo"),
            CssFragment::Class("bar"),
            CssFragment::Global(Global {
                inner: ".global",
                outer: ":global(.global)"
            }),
            CssFragment::Class("baz"),
            CssFragment::Class("moo"),
            CssFragment::Class("rad"),
        ])
    );

    assert!(input.is_empty());
}

#[test]
fn test_at_rule_simple() {
    let mut input = "@simple-rule blah \"asd;asd\" blah;";

    let r = at_rule.parse_next(&mut input);
    assert_eq!(r, Ok(vec![]));

    assert!(input.is_empty());
}

#[test]
fn test_at_rule_unknown() {
    let mut input = "@unknown blah \"asdasd\" blah {
        bunch of stuff {
            // things inside {
            blah
            ' { '
        }

        .bar {
            color: blue;

            .baz {
                color: green;
            }
        }
    }";

    let r = at_rule.parse_next(&mut input);
    assert_eq!(r, Ok(vec![]));

    assert!(input.is_empty());
}

#[test]
fn test_at_rule_media() {
    let mut input = "@media blah \"asdasd\" blah {
        .foo {
            background-color: red;
        }

        .bar {
            color: blue;

            .baz {
                color: green;
            }
        }
    }";

    let r = at_rule.parse_next(&mut input);
    assert_eq!(
        r,
        Ok(vec![
            CssFragment::Class("foo"),
            CssFragment::Class("bar"),
            CssFragment::Class("baz")
        ])
    );

    assert!(input.is_empty());
}
