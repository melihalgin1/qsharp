// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use super::{keyword::Keyword, scan::Scanner, ty::ty, ErrorKind, Parser, Result};
use crate::lex::{Delim, TokenKind};
use qsc_ast::ast::{Ident, NodeId, Pat, PatKind, Path, Span};
use std::str::FromStr;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FinalSep {
    Present,
    Missing,
}

impl FinalSep {
    pub(super) fn reify<T, U>(
        self,
        mut xs: Vec<T>,
        mut as_paren: impl FnMut(T) -> U,
        mut as_seq: impl FnMut(Vec<T>) -> U,
    ) -> U {
        if self == Self::Missing && xs.len() == 1 {
            as_paren(xs.pop().expect("Vector should have exactly one item."))
        } else {
            as_seq(xs)
        }
    }
}

pub(super) fn token(s: &mut Scanner, kind: TokenKind) -> Result<()> {
    if s.peek().kind == kind {
        s.advance();
        Ok(())
    } else {
        Err(s.error(ErrorKind::Token(kind)))
    }
}

pub(super) fn keyword(s: &mut Scanner, kw: Keyword) -> Result<()> {
    if s.peek().kind == TokenKind::Ident && s.read() == kw.as_str() {
        s.advance();
        Ok(())
    } else {
        Err(s.error(ErrorKind::Keyword(kw)))
    }
}

pub(super) fn ident(s: &mut Scanner) -> Result<Ident> {
    if s.peek().kind != TokenKind::Ident || Keyword::from_str(s.read()).is_ok() {
        return Err(s.error(ErrorKind::Rule("identifier")));
    }

    let span = s.peek().span;
    let name = s.read().to_string();
    s.advance();
    Ok(Ident {
        id: NodeId::PLACEHOLDER,
        span,
        name,
    })
}

pub(super) fn dot_ident(s: &mut Scanner) -> Result<Ident> {
    let p = path(s)?;
    let name = p.namespace.map_or(String::new(), |i| i.name + ".") + &p.name.name;
    Ok(Ident {
        id: p.id,
        span: p.span,
        name,
    })
}

pub(super) fn path(s: &mut Scanner) -> Result<Path> {
    let lo = s.peek().span.lo;
    let mut parts = vec![ident(s)?];
    while token(s, TokenKind::Dot).is_ok() {
        parts.push(ident(s)?);
    }

    let name = parts.pop().expect("Path has at least one part.");
    let namespace = match (parts.first(), parts.last()) {
        (Some(first), Some(last)) => {
            let lo = first.span.lo;
            let hi = last.span.hi;
            Some(Ident {
                id: NodeId::PLACEHOLDER,
                span: Span { lo, hi },
                name: join(parts.iter().map(|i| &i.name), "."),
            })
        }
        _ => None,
    };

    Ok(Path {
        id: NodeId::PLACEHOLDER,
        span: s.span(lo),
        namespace,
        name,
    })
}

pub(super) fn pat(s: &mut Scanner) -> Result<Pat> {
    let lo = s.peek().span.lo;
    let kind = if keyword(s, Keyword::Underscore).is_ok() {
        let ty = if token(s, TokenKind::Colon).is_ok() {
            Some(ty(s)?)
        } else {
            None
        };
        Ok(PatKind::Discard(ty))
    } else if token(s, TokenKind::DotDotDot).is_ok() {
        Ok(PatKind::Elided)
    } else if token(s, TokenKind::Open(Delim::Paren)).is_ok() {
        let (pats, final_sep) = seq(s, pat)?;
        token(s, TokenKind::Close(Delim::Paren))?;
        Ok(final_sep.reify(pats, |p| PatKind::Paren(Box::new(p)), PatKind::Tuple))
    } else if let Some(name) = opt(s, ident)? {
        let ty = if token(s, TokenKind::Colon).is_ok() {
            Some(ty(s)?)
        } else {
            None
        };
        Ok(PatKind::Bind(name, ty))
    } else {
        Err(s.error(ErrorKind::Rule("pattern")))
    }?;

    Ok(Pat {
        id: NodeId::PLACEHOLDER,
        span: s.span(lo),
        kind,
    })
}

pub(super) fn opt<T>(s: &mut Scanner, mut p: impl Parser<T>) -> Result<Option<T>> {
    let offset = s.peek().span.lo;
    match p(s) {
        Ok(x) => Ok(Some(x)),
        Err(_) if offset == s.peek().span.lo => Ok(None),
        Err(err) => Err(err),
    }
}

pub(super) fn many<T>(s: &mut Scanner, mut p: impl Parser<T>) -> Result<Vec<T>> {
    let mut xs = Vec::new();
    while let Some(x) = opt(s, &mut p)? {
        xs.push(x);
    }
    Ok(xs)
}

pub(super) fn seq<T>(s: &mut Scanner, mut p: impl Parser<T>) -> Result<(Vec<T>, FinalSep)> {
    let mut xs = Vec::new();
    let mut final_sep = FinalSep::Missing;
    while let Some(x) = opt(s, &mut p)? {
        xs.push(x);
        if token(s, TokenKind::Comma).is_ok() {
            final_sep = FinalSep::Present;
        } else {
            final_sep = FinalSep::Missing;
            break;
        }
    }
    Ok((xs, final_sep))
}

fn join(mut strings: impl Iterator<Item = impl AsRef<str>>, sep: &str) -> String {
    let mut string = String::new();
    if let Some(s) = strings.next() {
        string.push_str(s.as_ref());
    }
    for s in strings {
        string.push_str(sep);
        string.push_str(s.as_ref());
    }
    string
}
