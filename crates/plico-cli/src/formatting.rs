//! Component formatting is deliberately confined to skipped component functions.
use crate::{Error, IoSnafu, MetadataSnafu, Result};
use snafu::ResultExt;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use tree_sitter::{Node, Parser, Tree};

const SKIP: &[u8] = b"#[rustfmt::skip]";
const MARKER: &[u8] = b"#[doc = \"__plico_component_format_skip__\"]";

type Edit = (usize, usize, Vec<u8>);
struct Component<'t> {
    node: Node<'t>,
    skip: Node<'t>,
}
pub struct Options {
    pub check: bool,
    pub package: Option<String>,
    pub all: bool,
    pub width: usize,
    pub paths: Vec<PathBuf>,
}
struct Formatter<'p> {
    parser: Parser,
    path: &'p Path,
    edition: &'p str,
    width: usize,
}
fn failure(path: &Path, message: impl Into<String>) -> Error {
    Error::Format {
        path: path.into(),
        message: message.into(),
    }
}
fn nodes(root: Node<'_>) -> Vec<Node<'_>> {
    fn visit<'t>(node: Node<'t>, out: &mut Vec<Node<'t>>) {
        out.push(node);
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            visit(child, out);
        }
    }
    let mut result = Vec::new();
    visit(root, &mut result);
    result
}
fn compact(bytes: &[u8]) -> Vec<u8> {
    bytes
        .iter()
        .copied()
        .filter(|b| !b.is_ascii_whitespace())
        .collect()
}
fn is_component(node: Node<'_>, source: &[u8]) -> bool {
    let mut previous = node.prev_named_sibling();
    while let Some(attr) = previous {
        match attr.kind() {
            "attribute_item" => {
                let text = compact(&source[attr.byte_range()]);
                if text == b"#[component]"
                    || text == b"#[plico::component]"
                    || text == b"#[plico::component]"
                {
                    return true;
                }
            }
            "line_comment" | "block_comment" => {}
            _ => break,
        }
        previous = attr.prev_named_sibling();
    }
    false
}
fn components<'t>(tree: &'t Tree, source: &[u8], masked: bool) -> Vec<Component<'t>> {
    let mut result = Vec::new();
    for node in nodes(tree.root_node())
        .into_iter()
        .filter(|n| n.kind() == "function_item")
    {
        let mut previous = node.prev_named_sibling();
        let (mut component, mut skip) = (false, None);
        while let Some(attr) = previous {
            match attr.kind() {
                "attribute_item" => {
                    let text = compact(&source[attr.byte_range()]);
                    component |= text == b"#[component]"
                        || text == b"#[plico::component]"
                        || text == b"#[plico::component]";
                    if text == compact(if masked { MARKER } else { SKIP }) {
                        skip = Some(attr);
                    }
                }
                "line_comment" | "block_comment" => {}
                _ => break,
            }
            previous = attr.prev_named_sibling();
        }
        if component && let Some(skip) = skip {
            result.push(Component { node, skip });
        }
    }
    result.sort_by_key(|c| c.node.start_byte());
    result
}
fn markers(tree: &Tree) -> Vec<(Node<'_>, Node<'_>, Node<'_>)> {
    let mut result = Vec::new();
    for node in nodes(tree.root_node())
        .into_iter()
        .filter(|n| n.kind() == "block")
    {
        let mut cursor = node.walk();
        let children: Vec<_> = node
            .named_children(&mut cursor)
            .filter(|n| !matches!(n.kind(), "line_comment" | "block_comment"))
            .collect();
        for three in children.windows(3) {
            let (head, empty, body) = (three[0], three[1], three[2]);
            let block = if body.kind() == "block" {
                Some(body)
            } else {
                let mut cursor = body.walk();
                body.named_children(&mut cursor)
                    .find(|n| n.kind() == "block")
            };
            if head.kind() == "expression_statement"
                && empty.kind() == "empty_statement"
                && let Some(block) = block
            {
                result.push((head, empty, block));
            }
        }
    }
    result.sort_by_key(|(_, empty, _)| empty.start_byte());
    result
}
fn edit(source: &mut Vec<u8>, mut edits: Vec<Edit>) {
    edits.sort_by_key(|(start, end, _)| (*start, *end));
    for (start, end, replacement) in edits.into_iter().rev() {
        source.splice(start..end, replacement);
    }
}
impl<'p> Formatter<'p> {
    fn new(path: &'p Path, edition: &'p str, width: usize) -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .map_err(|e| failure(path, e.to_string()))?;
        Ok(Self {
            parser,
            path,
            edition,
            width,
        })
    }
    fn parse(&mut self, source: &[u8]) -> Result<Tree> {
        let tree = self
            .parser
            .parse(source, None)
            .ok_or_else(|| failure(self.path, "parser cancelled"))?;
        if tree.root_node().has_error() {
            let bad = nodes(tree.root_node())
                .into_iter()
                .find(|n| n.is_error() || n.is_missing())
                .unwrap_or(tree.root_node());
            return Err(failure(
                self.path,
                format!(
                    "invalid Rust at {}:{}; no files written",
                    bad.start_position().row + 1,
                    bad.start_position().column + 1
                ),
            ));
        }
        Ok(tree)
    }
    fn rustfmt(&self, source: &[u8]) -> Result<Vec<u8>> {
        let program = std::env::var_os("RUSTFMT").unwrap_or_else(|| "rustfmt".into());
        let mut child = Command::new(program)
            .args([
                "--edition",
                self.edition,
                "--emit",
                "stdout",
                "--config",
                "skip_children=true",
            ])
            .current_dir(self.path.parent().unwrap())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| failure(self.path, format!("cannot run rustfmt: {e}")))?;
        // Feed stdin concurrently: rustfmt may produce stdout while input exceeds a pipe buffer.
        let mut stdin = child.stdin.take().unwrap();
        let output = std::thread::scope(|scope| {
            let writer = scope.spawn(move || stdin.write_all(source));
            let output = child.wait_with_output();
            let written = writer.join().expect("rustfmt input writer panicked");
            (output, written)
        });
        let (output, written) = output;
        let output = output.context(IoSnafu)?;
        if !output.status.success() {
            return Err(failure(
                self.path,
                String::from_utf8_lossy(&output.stderr).trim(),
            ));
        }
        written.context(IoSnafu)?;
        Ok(output.stdout)
    }
    /// Pack multiline argument lists onto one line when they fit, or wrap
    /// them to as few lines as the width allows. rustfmt's one-argument-per-
    /// line layout is kept as the fallback when even a single argument
    /// overflows a continuation line.
    fn pack_arguments(&mut self, mut source: Vec<u8>) -> Result<Vec<u8>> {
        loop {
            let tree = self.parse(&source)?;
            let starts: Vec<usize> = nodes(tree.root_node())
                .into_iter()
                .filter(|n| n.kind() == "arguments")
                .filter(|n| source[n.byte_range()].contains(&b'\n'))
                .map(|n| n.start_byte())
                .collect();
            let mut changed = false;
            // Right to left, reparsing after each edit, so nested calls pack
            // before the calls that contain them.
            for start in starts.iter().rev() {
                let tree = self.parse(&source)?;
                let Some(args) = nodes(tree.root_node())
                    .into_iter()
                    .find(|n| n.kind() == "arguments" && n.start_byte() == *start)
                else {
                    continue;
                };
                if let Some(replacement) = self.packed_arguments(&source, args)
                    && replacement.as_slice() != &source[args.byte_range()]
                {
                    source.splice(args.byte_range(), replacement);
                    changed = true;
                    break;
                }
            }
            if !changed {
                return Ok(source);
            }
        }
    }
    fn packed_arguments(&self, source: &[u8], args: Node<'_>) -> Option<Vec<u8>> {
        // Only call-style argument lists; other argument holders keep their
        // layout.
        let call = args.parent()?;
        call.child_by_field_name("function")?;
        for node in nodes(args) {
            match node.kind() {
                "line_comment" | "block_comment" => return None,
                "raw_string_literal" | "string_literal" | "char_literal"
                    if source[node.byte_range()].contains(&b'\n') =>
                {
                    return None;
                }
                _ => {}
            }
        }
        let mut cursor = args.walk();
        let items: Vec<_> = args.named_children(&mut cursor).collect();
        if items.is_empty() {
            return None;
        }
        if compact(&source[args.start_byte() + 1..items[0].start_byte()]) != b"" {
            return None;
        }
        let close = args.end_byte() - 1;
        let close_gap = compact(&source[items[items.len() - 1].end_byte()..close]);
        if close_gap != b"" && close_gap != b"," {
            return None;
        }
        let count = |text: &[u8]| String::from_utf8_lossy(text).chars().count();
        let line_start = source[..args.start_byte()]
            .iter()
            .rposition(|b| *b == b'\n')
            .map_or(0, |i| i + 1);
        let prefix = &source[line_start..args.start_byte()];
        let suffix = source[args.end_byte()..]
            .split(|b| *b == b'\n')
            .next()
            .unwrap_or(b"")
            .trim_ascii();
        // One line when everything fits.
        let mut joined: Vec<u8> = vec![b'('];
        for (index, item) in items.iter().enumerate() {
            let text = &source[item.byte_range()];
            if text.contains(&b'\n') {
                // An inner multiline argument must pack first (or never).
                return None;
            }
            if index > 0 && compact(&source[items[index - 1].end_byte()..item.start_byte()]) != b","
            {
                return None;
            }
            if index > 0 {
                joined.extend_from_slice(b", ");
            }
            joined.extend_from_slice(text);
        }
        joined.push(b')');
        if count(prefix) + count(&joined) + count(suffix) <= self.width {
            return Some(joined);
        }
        // Greedy fill: continuation lines one indent past the call.
        let mut continuation: Vec<u8> = prefix
            .iter()
            .copied()
            .take_while(|b| matches!(b, b' ' | b'\t'))
            .collect();
        if continuation.contains(&b'\t') {
            continuation.push(b'\t');
        } else {
            continuation.extend_from_slice(b"    ");
        }
        let mut lines: Vec<Vec<u8>> = vec![Vec::new()];
        let mut room = self.width.saturating_sub(count(prefix) + 1);
        for item in &items {
            let mut piece = source[item.byte_range()].to_vec();
            piece.push(b',');
            let piece_len = count(&piece) + 1;
            let empty = lines.last().is_some_and(Vec::is_empty);
            if room < piece_len && (!empty || lines.len() == 1) {
                lines.push(Vec::new());
                room = self.width.saturating_sub(count(&continuation));
            }
            let line = lines.last_mut().unwrap();
            if !line.is_empty() {
                line.push(b' ');
            }
            line.extend_from_slice(&piece);
            room = room.saturating_sub(piece_len);
        }
        let last = lines.last_mut().unwrap();
        while matches!(last.last(), Some(b',')) {
            last.pop();
        }
        last.push(b')');
        if count(prefix) + 1 + count(&lines[0]) > self.width {
            return None;
        }
        for line in lines.iter().skip(1) {
            if count(&continuation) + count(line) > self.width {
                return None;
            }
        }
        if lines
            .last()
            .is_some_and(|line| count(line) + count(suffix) > self.width)
        {
            return None;
        }
        let mut out = vec![b'('];
        out.extend_from_slice(&lines[0]);
        for line in &lines[1..] {
            out.push(b'\n');
            out.extend_from_slice(&continuation);
            out.extend_from_slice(line);
        }
        Some(out)
    }
    fn align_arguments(&mut self, mut source: Vec<u8>) -> Result<Vec<u8>> {
        // Token order stays fixed across whitespace edits. Align each argument
        // list once so shared closing lines cannot oscillate between parents.
        let mut aligned = BTreeSet::new();
        loop {
            let tree = self.parse(&source)?;
            let all = nodes(tree.root_node());
            let opaque: Vec<_> = all
                .iter()
                .filter(|node| {
                    matches!(
                        node.kind(),
                        "raw_string_literal" | "string_literal" | "char_literal" | "block_comment"
                    )
                })
                .map(Node::byte_range)
                .collect();
            let mut changed = false;
            for (index, args) in all.iter().filter(|n| n.kind() == "arguments").enumerate() {
                if aligned.contains(&index) {
                    continue;
                }
                let Some(call) = args.parent() else { continue };
                let Some(function) = call.child_by_field_name("function") else {
                    continue;
                };
                if !matches!(
                    function.kind(),
                    "field_expression" | "generic_function" | "identifier" | "scoped_identifier"
                ) {
                    continue;
                }
                let body = &source[args.byte_range()];
                if !body.contains(&b'\n') {
                    continue;
                }
                let close = args.end_byte() - 1;
                if source
                    .get(close + 1)
                    .is_some_and(|b| matches!(b, b')' | b']' | b'}'))
                {
                    continue;
                }
                let close_line = source[..close]
                    .iter()
                    .rposition(|b| *b == b'\n')
                    .map_or(0, |i| i + 1);
                let prefix = &source[close_line..close];
                if !prefix
                    .iter()
                    .all(|b| matches!(b, b' ' | b'\t' | b'}' | b']' | b')'))
                {
                    continue;
                }
                let old_indent = prefix
                    .iter()
                    .take_while(|b| matches!(b, b' ' | b'\t'))
                    .count();
                let head = source[..function.start_byte()]
                    .rsplit(|b| *b == b'\n')
                    .next()
                    .unwrap();
                let new_indent = head
                    .iter()
                    .take_while(|b| matches!(b, b' ' | b'\t'))
                    .count();
                let delta = new_indent as isize - old_indent as isize;
                if delta == 0 {
                    continue;
                }
                let starts: Vec<_> = source[args.start_byte()..close_line]
                    .iter()
                    .enumerate()
                    .filter(|(_, byte)| **byte == b'\n')
                    .map(|(offset, _)| args.start_byte() + offset + 1)
                    .collect();
                let mut edits = Vec::new();
                for start in starts {
                    if delta == 0 || opaque.iter().any(|range| range.contains(&start)) {
                        continue;
                    }
                    let indentation = source[start..]
                        .iter()
                        .take_while(|b| matches!(b, b' ' | b'\t'))
                        .count();
                    if source
                        .get(start + indentation)
                        .is_none_or(|b| matches!(b, b'\n' | b'\r'))
                    {
                        continue;
                    }
                    let Some(length) = indentation.checked_add_signed(delta) else {
                        return Err(failure(
                            self.path,
                            "cannot safely align multiline arguments",
                        ));
                    };
                    let unit = if source[start..start + indentation].contains(&b'\t') {
                        b'\t'
                    } else {
                        b' '
                    };
                    edits.push((start, start + indentation, vec![unit; length]));
                }
                if !edits.is_empty() {
                    aligned.insert(index);
                    edit(&mut source, edits);
                    changed = true;
                    break;
                }
            }
            if !changed {
                return Ok(source);
            }
        }
    }
    // Native Rust operator children (`+` + closure): whitespace only, never syntax.
    fn align_operator_children(&mut self, mut source: Vec<u8>) -> Result<Vec<u8>> {
        let mut aligned = BTreeSet::new();
        loop {
            let tree = self.parse(&source)?;
            let all = nodes(tree.root_node());
            let opaque: Vec<_> = all
                .iter()
                .filter(|n| {
                    matches!(
                        n.kind(),
                        "raw_string_literal" | "string_literal" | "char_literal" | "block_comment"
                    )
                })
                .map(Node::byte_range)
                .collect();
            let mut changed = false;
            for (index, binary) in all
                .iter()
                .filter(|n| n.kind() == "binary_expression")
                .enumerate()
            {
                if aligned.contains(&index) {
                    continue;
                }
                let (Some(left), Some(right)) = (
                    binary.child_by_field_name("left"),
                    binary.child_by_field_name("right"),
                ) else {
                    continue;
                };
                let operator = compact(&source[left.end_byte()..right.start_byte()]);
                if right.kind() != "closure_expression" || operator != b"+" {
                    continue;
                }
                let Some(body) = right
                    .child_by_field_name("body")
                    .filter(|n| n.kind() == "block")
                else {
                    continue;
                };
                let line_start = |offset: usize| {
                    source[..offset]
                        .iter()
                        .rposition(|b| *b == b'\n')
                        .map_or(0, |i| i + 1)
                };
                let indent =
                    |start: usize| source[start..].iter().take_while(|b| **b == b' ').count();
                let base = indent(line_start(binary.start_byte()));
                let old = indent(line_start(body.end_byte() - 1));
                let tail = &source[line_start(left.end_byte())..left.end_byte()];
                let header = &source[right.start_byte()..=body.start_byte()];
                let fits = !header.contains(&b'\n')
                    && String::from_utf8_lossy(tail).chars().count()
                        + 3
                        + String::from_utf8_lossy(header).chars().count()
                        <= self.width;
                // Keep the native addition spelling for children.
                let gap = if fits {
                    b" + ".to_vec()
                } else {
                    let newline: &[u8] = if source.windows(2).any(|w| w == b"\r\n") {
                        b"\r\n"
                    } else {
                        b"\n"
                    };
                    [newline, &vec![b' '; base], b"+ "].concat()
                };
                let mut edits = Vec::new();
                if source[left.end_byte()..right.start_byte()] != gap {
                    edits.push((left.end_byte(), right.start_byte(), gap));
                }
                let delta = if source[body.byte_range()].contains(&b'\n') {
                    base as isize - old as isize
                } else {
                    0
                };
                if delta != 0 {
                    for (offset, byte) in source[right.byte_range()].iter().enumerate() {
                        if *byte != b'\n' {
                            continue;
                        }
                        let start = right.start_byte() + offset + 1;
                        if opaque.iter().any(|range| range.contains(&start)) {
                            continue;
                        }
                        let old = indent(start);
                        if source
                            .get(start + old)
                            .is_none_or(|b| matches!(b, b'\r' | b'\n'))
                        {
                            continue;
                        }
                        // rustfmt can place argument continuations flush left inside
                        // nested operator closures. Do not underflow while shifting
                        // the surrounding closure; the argument pass aligns them.
                        let new = old.saturating_add_signed(delta);
                        edits.push((start, start + old, vec![b' '; new]));
                    }
                }
                if !edits.is_empty() {
                    aligned.insert(index);
                    edit(&mut source, edits);
                    changed = true;
                    break; // Reparse after each closure so nested offsets stay valid.
                }
            }
            if !changed {
                return Ok(source);
            }
        }
    }
    fn align_operator_braces(&mut self, mut source: Vec<u8>) -> Result<Vec<u8>> {
        let tree = self.parse(&source)?;
        let all = nodes(tree.root_node());
        let newline: &[u8] = if source.windows(2).any(|w| w == b"\r\n") {
            b"\r\n"
        } else {
            b"\n"
        };
        let mut edits = Vec::new();
        for binary in all.iter().filter(|node| node.kind() == "binary_expression") {
            let (Some(left), Some(right)) = (
                binary.child_by_field_name("left"),
                binary.child_by_field_name("right"),
            ) else {
                continue;
            };
            let operator = compact(&source[left.end_byte()..right.start_byte()]);
            if right.kind() != "closure_expression" || operator != b"+" {
                continue;
            }
            let Some(body) = right
                .child_by_field_name("body")
                .filter(|node| node.kind() == "block")
            else {
                continue;
            };
            let mut end = body.start_byte();
            while end > right.start_byte() && source[end - 1].is_ascii_whitespace() {
                end -= 1;
            }
            // Ignore the old brace gap when deciding whether a short header can
            // collapse. A trailing line comment must keep the brace on a new line.
            let multiline = source[binary.start_byte()..end].contains(&b'\n')
                || all
                    .iter()
                    .any(|node| node.kind() == "line_comment" && node.end_byte() == end);
            let gap = if multiline {
                let line = source[..binary.start_byte()]
                    .iter()
                    .rposition(|b| *b == b'\n')
                    .map_or(0, |i| i + 1);
                let indent = source[line..]
                    .iter()
                    .take_while(|b| matches!(b, b' ' | b'\t'))
                    .count();
                [newline, &source[line..line + indent]].concat()
            } else {
                b" ".to_vec()
            };
            if source[end..body.start_byte()] != gap {
                edits.push((end, body.start_byte(), gap));
            }
        }
        edit(&mut source, edits);
        Ok(source)
    }

    fn align_if_braces(&mut self, mut source: Vec<u8>) -> Result<Vec<u8>> {
        let tree = self.parse(&source)?;
        let mut edits = Vec::new();
        for node in nodes(tree.root_node())
            .into_iter()
            .filter(|n| n.kind() == "if_expression")
        {
            let (Some(condition), Some(body)) = (
                node.child_by_field_name("condition"),
                node.child_by_field_name("consequence"),
            ) else {
                continue;
            };
            let gap = &source[condition.end_byte()..body.start_byte()];
            // Chain packing may have made a rustfmt-multiline condition fit on
            // one line. Bring its brace back too; comments remain boundaries.
            if source[node.start_byte()..condition.end_byte()].contains(&b'\n')
                || !gap.iter().all(u8::is_ascii_whitespace)
            {
                continue;
            }
            let line = source[..condition.end_byte()]
                .rsplit(|b| *b == b'\n')
                .next()
                .unwrap();
            if String::from_utf8_lossy(line).chars().count() + 2 <= self.width && gap != b" " {
                edits.push((condition.end_byte(), body.start_byte(), b" ".to_vec()));
            }
        }
        edit(&mut source, edits);
        Ok(source)
    }

    fn pack_conditionals(&mut self, mut source: Vec<u8>) -> Result<Vec<u8>> {
        // Work from right to left and reparse after each replacement so nested
        // expressions never leave stale byte offsets behind.
        let tree = self.parse(&source)?;
        let starts: Vec<_> = nodes(tree.root_node())
            .into_iter()
            .filter(|n| n.kind() == "if_expression")
            .map(|n| n.start_byte())
            .rev()
            .collect();
        for start in starts {
            let tree = self.parse(&source)?;
            let Some(node) = nodes(tree.root_node())
                .into_iter()
                .find(|n| n.kind() == "if_expression" && n.start_byte() == start)
            else {
                continue;
            };
            let descendants = nodes(node);
            if !source[node.byte_range()].contains(&b'\n')
                || node.child_by_field_name("alternative").is_none()
                || descendants.iter().any(|n| {
                    matches!(n.kind(), "line_comment" | "block_comment")
                        || (matches!(n.kind(), "string_literal" | "raw_string_literal")
                            && source[n.byte_range()].contains(&b'\n'))
                })
            {
                continue;
            }
            let blocks: Vec<_> = descendants.iter().filter(|n| n.kind() == "block").collect();
            if blocks.len() != 2
                || blocks.iter().any(|block| {
                    let mut cursor = block.walk();
                    let children: Vec<_> = block.named_children(&mut cursor).collect();
                    children.len() != 1
                        || matches!(
                            children[0].kind(),
                            "expression_statement" | "let_declaration"
                        )
                })
            {
                continue;
            }
            let mut candidate = source[node.byte_range()].to_vec();
            let leaves: Vec<_> = descendants
                .iter()
                .filter(|n| n.child_count() == 0)
                .collect();
            let mut edits = Vec::new();
            for pair in leaves.windows(2) {
                let a = pair[0].end_byte();
                let b = pair[1].start_byte();
                if source[a..b].contains(&b'\n') && source[a..b].iter().all(u8::is_ascii_whitespace)
                {
                    edits.push((a - start, b - start, b" ".to_vec()));
                }
            }
            edit(&mut candidate, edits);
            let prefix = source[..start].rsplit(|b| *b == b'\n').next().unwrap();
            let suffix = source[node.end_byte()..]
                .split(|b| *b == b'\n')
                .next()
                .unwrap();
            let length = [prefix, candidate.as_slice(), suffix]
                .into_iter()
                .map(|s| String::from_utf8_lossy(s).chars().count())
                .sum::<usize>();
            if length <= self.width {
                source.splice(node.byte_range(), candidate);
            }
        }
        Ok(source)
    }

    fn flatten(&mut self, mut source: Vec<u8>) -> Result<Vec<u8>> {
        let tree = self.parse(&source)?;
        // Packing a call onto one line drops its trailing comma, the same
        // normalization rustfmt performs; bare comma tokens are excluded so
        // the preservation check still compares every other leaf.
        let fingerprint = |tree: &Tree, source: &[u8]| -> Vec<Vec<u8>> {
            nodes(tree.root_node())
                .into_iter()
                .filter(|n| n.child_count() == 0)
                .filter(|n| source[n.byte_range()] != *b",")
                .map(|n| source[n.byte_range()].to_vec())
                .collect()
        };
        let before = fingerprint(&tree, &source);
        source = self.pack_conditionals(source)?;
        let tree = self.parse(&source)?;
        let mut gaps = Vec::new();
        for node in nodes(tree.root_node())
            .into_iter()
            .filter(|n| n.kind() == "field_expression")
        {
            let (Some(value), Some(field)) = (
                node.child_by_field_name("value"),
                node.child_by_field_name("field"),
            ) else {
                continue;
            };
            // Comments between chain segments are boundaries, never whitespace to remove.
            if compact(&source[value.end_byte()..field.start_byte()]) != b"." {
                continue;
            }
            let mut segment = node;
            while let Some(parent) = segment.parent() {
                if matches!(parent.kind(), "generic_function" | "call_expression")
                    && parent.child_by_field_name("function") == Some(segment)
                {
                    segment = parent;
                } else {
                    break;
                }
            }
            let prefix = source[..node.start_byte()]
                .rsplit(|b| *b == b'\n')
                .next()
                .unwrap();
            let mut indentation: Vec<_> = prefix
                .iter()
                .copied()
                .take_while(|b| matches!(b, b' ' | b'\t'))
                .collect();
            if indentation.contains(&b'\t') {
                indentation.push(b'\t');
            } else {
                indentation.extend_from_slice(b"    ");
            }
            // Account for a statement terminator / marker when packing the final segment.
            let mut end = segment.end_byte();
            while end < source.len()
                && matches!(source[end], b';' | b',' | b')' | b'?' | b'{' | b' ')
            {
                end += 1;
            }
            gaps.push((value.end_byte(), field.start_byte(), end, indentation));
        }
        gaps.sort_by_key(|(start, _, _, _)| *start);
        let mut shift = 0isize;
        for (start, end, segment_end, indentation) in gaps {
            let (start, end, segment_end) = (
                start.checked_add_signed(shift).unwrap(),
                end.checked_add_signed(shift).unwrap(),
                segment_end.checked_add_signed(shift).unwrap(),
            );
            let left = source[..start].rsplit(|b| *b == b'\n').next().unwrap();
            let right = source[end..segment_end]
                .split(|b| *b == b'\n')
                .next()
                .unwrap();
            let length = String::from_utf8_lossy(left).chars().count()
                + 1
                + String::from_utf8_lossy(right).trim_end().chars().count();
            let replacement = if length <= self.width {
                b".".to_vec()
            } else {
                [b"\n".as_slice(), &indentation, b"."].concat()
            };
            shift += replacement.len() as isize - (end - start) as isize;
            source.splice(start..end, replacement);
        }
        source = self.pack_arguments(source)?;
        source = self.align_arguments(source)?;
        source = self.align_operator_children(source)?;
        source = self.align_arguments(source)?;
        source = self.align_operator_braces(source)?;
        source = self.align_if_braces(source)?;
        let tree = self.parse(&source)?;
        if fingerprint(&tree, &source) != before {
            return Err(failure(self.path, "token preservation check failed"));
        }
        Ok(source)
    }
    fn format(&mut self, source: &[u8]) -> Result<Vec<u8>> {
        let tree = self.parse(source)?;
        let selected = components(&tree, source, false);
        // Reject retired syntax before rustfmt can erase its empty statement.
        for (_, empty, _) in markers(&tree) {
            let mut parent = empty.parent();
            while let Some(node) = parent {
                if node.kind() == "function_item" {
                    if is_component(node, source) {
                        return Err(failure(
                            self.path,
                            "layout;; children was removed; use layout + |cx| { ... }; no files written",
                        ));
                    }
                    break;
                }
                parent = node.parent();
            }
        }
        if selected.is_empty() {
            return self.rustfmt(source);
        }
        if source.windows(MARKER.len()).any(|w| w == MARKER) {
            return Err(failure(
                self.path,
                "reserved formatter marker already present",
            ));
        }
        if selected
            .windows(2)
            .any(|w| w[0].node.end_byte() > w[1].node.start_byte())
        {
            return Err(failure(
                self.path,
                "nested skipped components are not supported",
            ));
        }
        let edits: Vec<_> = selected
            .iter()
            .map(|c| (c.skip.start_byte(), c.skip.end_byte(), MARKER.to_vec()))
            .collect();
        let mut masked = source.to_vec();
        edit(&mut masked, edits);
        let formatted = self.rustfmt(&masked)?;
        let formatted_tree = self.parse(&formatted)?;
        let new = components(&formatted_tree, &formatted, true);
        if new.len() != selected.len() {
            return Err(failure(self.path, "marked function count changed"));
        }
        let mut result = source.to_vec();
        let mut edits = Vec::new();
        for (original, new) in selected.iter().zip(new) {
            let fragment = self.flatten(formatted[new.node.byte_range()].to_vec())?;
            edits.push((
                original.node.start_byte(),
                original.node.end_byte(),
                fragment,
            ));
        }
        edit(&mut result, edits);
        self.parse(&result)?;
        // Restore the real skip attributes before formatting ordinary Rust outside components.
        self.rustfmt(&result)
    }
}

fn collect(path: &Path, files: &mut BTreeSet<PathBuf>) -> Result<()> {
    let metadata = fs::symlink_metadata(path).context(IoSnafu)?;
    if metadata.file_type().is_symlink() {
        return Err(failure(path, "symbolic links are not formatted"));
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path).context(IoSnafu)? {
            let entry = entry.context(IoSnafu)?;
            if matches!(
                entry.file_name().to_str(),
                Some("target" | ".git" | "node_modules" | "reports")
            ) || entry.file_type().context(IoSnafu)?.is_symlink()
            {
                continue;
            }
            collect(&entry.path(), files)?;
        }
    } else if path.extension().is_some_and(|ext| ext == "rs") {
        files.insert(path.canonicalize().context(IoSnafu)?);
    }
    Ok(())
}
pub fn run(directory: &Path, manifest: &Path, options: Options) -> Result<u8> {
    if !(20..=1000).contains(&options.width) {
        return Err(failure(manifest, "--width must be between 20 and 1000"));
    }
    if options.all && options.package.is_some() {
        return Err(failure(manifest, "--all and --package cannot be combined"));
    }
    let metadata = cargo_metadata::MetadataCommand::new()
        .cargo_path(crate::cargo())
        .manifest_path(manifest)
        .no_deps()
        .exec()
        .context(MetadataSnafu)?;
    let packages: Vec<_> = metadata
        .packages
        .iter()
        .filter(|p| metadata.workspace_members.contains(&p.id))
        .collect();
    if let Some(name) = &options.package
        && !packages.iter().any(|p| p.name.as_str() == name)
    {
        return Err(failure(
            manifest,
            format!("no workspace package named {name}"),
        ));
    }
    let mut files = BTreeSet::new();
    if options.paths.is_empty() {
        for package in &packages {
            if options
                .package
                .as_ref()
                .is_some_and(|name| name != package.name.as_str())
            {
                continue;
            }
            let root = package.manifest_path.parent().unwrap().as_std_path();
            for name in ["src", "tests", "examples", "benches"] {
                let path = root.join(name);
                if path.exists() {
                    collect(&path, &mut files)?;
                }
            }
            let build = root.join("build.rs");
            if build.exists() {
                collect(&build, &mut files)?;
            }
            for target in &package.targets {
                collect(target.src_path.as_std_path(), &mut files)?;
            }
        }
    } else {
        if options.package.is_some() {
            return Err(failure(
                manifest,
                "explicit paths and --package cannot be combined",
            ));
        }
        for path in &options.paths {
            collect(&directory.join(path), &mut files)?;
        }
    }
    if files.is_empty() && !options.paths.is_empty() {
        return Err(failure(manifest, "no Rust files selected"));
    }
    let roots: BTreeMap<_, _> = packages
        .iter()
        .map(|p| {
            (
                p.manifest_path.parent().unwrap().as_std_path(),
                p.edition.as_str(),
            )
        })
        .collect();
    let mut changed = Vec::new();
    // Finish every parse/format before writing anything, including check mode.
    for path in files {
        let edition = roots
            .iter()
            .filter(|(root, _)| path.starts_with(root))
            .max_by_key(|(root, _)| root.components().count())
            .map_or("2024", |(_, edition)| *edition);
        let original = fs::read(&path).context(IoSnafu)?;
        let formatted = Formatter::new(&path, edition, options.width)?.format(&original)?;
        if original != formatted {
            changed.push((path, original, formatted));
        }
    }
    if options.check {
        for (path, _, _) in &changed {
            eprintln!("would format {}", path.display());
        }
        return Ok(u8::from(!changed.is_empty()));
    }
    for (path, original, _) in &changed {
        if fs::read(path).context(IoSnafu)? != *original {
            return Err(failure(
                path,
                "file changed during formatting; no files written",
            ));
        }
    }
    for (path, _, formatted) in &changed {
        let permissions = fs::metadata(path).context(IoSnafu)?.permissions();
        let mut temporary =
            tempfile::NamedTempFile::new_in(path.parent().unwrap()).context(IoSnafu)?;
        temporary
            .as_file()
            .set_permissions(permissions)
            .context(IoSnafu)?;
        temporary.write_all(formatted).context(IoSnafu)?;
        temporary
            .persist(path)
            .map_err(|e| failure(path, e.to_string()))?;
        println!("formatted {}", path.display());
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = r###"use plico::prelude::*;
fn ordinary( ){let n=1;}
#[component]
#[rustfmt::skip]
fn screen(cx: &mut UiContext) {
    column(cx)
        .padding(20)
        .gap(8) + |cx| {
            // Keep ;; { in comments and raw strings.
            text(cx, r#"literal ;; { .children( }"#);
            row(cx)
                .gap(8) + |cx| /* marker comment */  {
                    button(cx).text("Save").primary();
                };
            match true { true => {text(cx,"yes");} false => {} }
        };
}
#[rustfmt::skip]
fn unrelated( ) { let n= 2; }
"###;

    #[test]
    fn nested_operator_children_with_callback_arguments_are_stable() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested.rs");
        let input = br#"#[component]
#[rustfmt::skip]
fn screen(cx: &mut UiContext) {
    column(cx).grow(1.).min_height(0.).padding_x(28).padding_y(8).gap(16).scroll_y(true).smooth_scroll(true) + |cx| {
        match page {
            0 => virtual_list(cx, items.len(), 94.).height(430.).smooth_scroll(true).rows(
                |index| items[index].id,
                |cx, index| {
                    row(cx).height(94.).gap(14).padding(10).align(Align::Center) + |cx| {
                        column(cx).gap(5).grow(1.) + |cx| {
                            heading(cx, &items[index].title, "A long description for nested content");
                            text(cx, &items[index].destination).font_size(14).weight(650);
                        };
                    };
                },
            ),
            _ => {
                column(cx).max_width(920.).padding(22).gap(14).radius(18.).background(Color::Panel).border(1., Color::Border) + |cx| {
                    heading(cx, "Appearance and network", "Your local port and display preferences.");
                };
            }
        }
    };
}
"#;
        for width in [60, 80, 120] {
            let mut formatter = Formatter::new(&path, "2024", width).unwrap();
            let once = formatter.format(input).unwrap();
            assert_eq!(formatter.format(&once).unwrap(), once);
        }
    }

    #[test]
    fn argument_lists_pack_to_one_line_or_wrap_within_the_width() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("arguments.rs");
        let input = br#"#[component]
#[rustfmt::skip]
fn screen(cx: &mut UiContext) {
    if icon_button(
        cx,
        "Restore projects",
        &crate::icons::ARCHIVE_RESTORE,
        true,
        false,
    )
    .clicked()
    {
        work();
    }
}
"#;
        let mut formatter = Formatter::new(&path, "2024", 120).unwrap();
        let once = formatter.format(input).unwrap();
        let output = String::from_utf8_lossy(&once);
        assert!(
            output.contains(
                "if icon_button(cx, \"Restore projects\", &crate::icons::ARCHIVE_RESTORE, true, false).clicked()"
            ),
            "{output}"
        );
        assert_eq!(formatter.format(&once).unwrap(), once);

        // Below the width the same call wraps greedily instead of stacking
        // one argument per line.
        let mut narrow = Formatter::new(&path, "2024", 72).unwrap();
        let wrapped = narrow.format(input).unwrap();
        let output = String::from_utf8_lossy(&wrapped);
        assert!(
            output.contains("if icon_button(cx, \"Restore projects\",\n        &crate::icons::ARCHIVE_RESTORE, true, false).clicked()"),
            "{output}"
        );
        assert_eq!(narrow.format(&wrapped).unwrap(), wrapped);

        // A single argument that cannot fit a line keeps the vertical layout.
        let long = br#"#[component]
#[rustfmt::skip]
fn screen(cx: &mut UiContext) {
    if icon_button(
        cx,
        "Restore every archived project including the ones hidden by the filter pane",
        &crate::icons::ARCHIVE_RESTORE,
        true,
        false,
    )
    .clicked()
    {
        work();
    }
}
"#;
        let mut tiny = Formatter::new(&path, "2024", 60).unwrap();
        let kept = tiny.format(long).unwrap();
        assert_eq!(tiny.format(&kept).unwrap(), kept);
    }

    #[test]
    fn packed_if_conditions_keep_their_brace_on_the_same_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("counter.rs");
        let input = r#"#[component]
#[rustfmt::skip]
fn counter(cx: &mut UiContext, count: &mut i32) {
    if button(cx).text("-").aria_label("Decrement").ghost().clicked()
    {
        *count -= 1;
    }
    if button(cx).text("+").aria_label("Increment").primary().clicked()
    {
        *count += 1;
    }
    if ready // keep this comment
    {
        work();
    }
}
"#;
        for width in [60, 120] {
            let mut formatter = Formatter::new(&path, "2024", width).unwrap();
            let once = formatter.format(input.as_bytes()).unwrap();
            let output = String::from_utf8_lossy(&once);
            if width == 120 {
                assert!(output.contains(".ghost().clicked() {"), "{output}");
                assert!(output.contains(".primary().clicked() {"), "{output}");
            }
            assert!(
                output.contains("// keep this comment\n") && !output.contains("if ready {"),
                "{output}"
            );
            assert_eq!(formatter.format(&once).unwrap(), once);
        }
    }

    #[test]
    fn simple_conditionals_pack_only_when_the_line_fits() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conditional.rs");
        let input = b"#[component]\n#[rustfmt::skip]\nfn screen(cx: &mut UiContext) {\n    text(cx, if self.over { 0xeee4da } else { 0xedcf72 });\n    text(cx, if flag { work(); 1 } else { 2 });\n}\n";
        for width in [60, 80, 120] {
            let mut formatter = Formatter::new(&path, "2024", width).unwrap();
            let once = formatter.format(input).unwrap();
            let output = String::from_utf8_lossy(&once);
            if width >= 80 {
                assert!(
                    output.contains("if self.over { 0xeee4da } else { 0xedcf72 }"),
                    "{output}"
                );
            }
            assert_eq!(formatter.format(&once).unwrap(), once);
        }
    }

    #[test]
    fn gallery_nested_layouts_format_idempotently() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gallery.rs");
        let mut formatter = Formatter::new(&path, "2024", 120).unwrap();
        let input = include_bytes!("../../../apps/gallery/src/layout_demo.rs");
        let once = formatter.format(input).unwrap();
        assert_eq!(formatter.format(&once).unwrap(), once);
    }

    #[test]
    fn native_operator_children_are_flat_and_idempotent() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("operator.rs");
        let source = br###"#[component]
#[rustfmt::skip]
fn screen(cx: &mut UiContext) {
    let bits = 1 << 3;
    let padding = 16 + 4;
    column(cx).padding(20).gap(8) + |cx| {
        text(cx, "literal + |cx| { stays intact");
        row(cx).width(200.).height(100.).padding(16).gap(12).radius(8.).grow(1.).shrink(0.) + |cx| {
            text(cx, r#"raw
   unchanged
string"#);
        };
        row(cx) /* retain comment */ + |cx| { text(cx, "comment"); };
    };
}
"###;
        for width in [60, 80, 120] {
            let mut formatter = Formatter::new(&path, "2024", width).unwrap();
            let formatted = formatter.format(source).unwrap();
            assert_eq!(formatter.format(&formatted).unwrap(), formatted);
            let text = String::from_utf8(formatted).unwrap();
            assert!(
                text.contains("column(cx).padding(20).gap(8) + |cx| {"),
                "{text}"
            );
            assert!(text.contains("        text(cx, \"literal"), "{text}");
            assert!(text.contains("raw\n   unchanged\nstring"), "{text}");
            assert!(text.contains("/* retain comment */"));
            assert!(text.contains("let bits = 1 << 3;"));
            assert!(text.contains("let padding = 16 + 4;"));
        }
    }

    #[test]
    fn component_operators_comments_and_rustfmt_compose_at_multiple_widths() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sample.rs");
        for width in [60, 80, 120] {
            let mut formatter = Formatter::new(&path, "2024", width).unwrap();
            let formatted = formatter.format(SOURCE.as_bytes()).unwrap();
            assert_eq!(formatter.format(&formatted).unwrap(), formatted);
            let text = String::from_utf8(formatted.clone()).unwrap();
            assert!(text.contains("fn ordinary() {\n    let n = 1;\n}"));
            assert!(text.contains("column(cx).padding(20).gap(8) + |cx| {"));
            assert!(text.contains("/* marker comment */"));
            assert!(text.contains(r##"r#"literal ;; { .children( }"#"##));
            assert!(text.contains("fn unrelated( ) { let n= 2; }"));
            assert!(!text.contains("__plico_double_semi_marker"));
            let tree = formatter.parse(&formatted).unwrap();
            assert_eq!(markers(&tree).len(), 0);
            assert_eq!(formatter.rustfmt(&formatted).unwrap(), formatted);
        }
    }

    #[test]
    fn native_children_and_namespaced_components_work_inside_impls() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sample.rs");
        let source = br#"struct Demo;
impl Demo {
#[plico::component]
#[rustfmt::skip]
fn screen(cx: &mut UiContext) {
    column(cx).padding(20).gap(8).children(|inner| {
        text(inner, "hello");
    });
}
}"#;
        let mut formatter = Formatter::new(&path, "2024", 120).unwrap();
        let formatted = formatter.format(source).unwrap();
        assert_eq!(formatter.format(&formatted).unwrap(), formatted);
        assert!(
            String::from_utf8_lossy(&formatted)
                .contains("column(cx).padding(20).gap(8).children(|inner| {")
        );
        let canonical = String::from_utf8_lossy(source).replace("plico::component", "component");
        let formatted = formatter.format(canonical.as_bytes()).unwrap();
        assert_eq!(formatter.format(&formatted).unwrap(), formatted);
        assert!(
            String::from_utf8_lossy(&formatted)
                .contains("column(cx).padding(20).gap(8).children(|inner| {")
        );
    }

    #[test]
    fn long_chains_pack_multiple_methods_on_every_line() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sample.rs");
        let source = br#"#[component]
#[rustfmt::skip]
fn screen(cx: &mut UiContext) {
    column(cx).padding(16).gap(8).width(320.).background(Color::Panel).radius(12.).border(1., Color::Border).height(100.).grow(1.) + |cx| {
        text(cx, "hello");
    };
}"#;
        for width in [60, 80, 120] {
            let mut formatter = Formatter::new(&path, "2024", width).unwrap();
            let formatted = formatter.format(source).unwrap();
            assert_eq!(formatter.format(&formatted).unwrap(), formatted);
            let text = String::from_utf8_lossy(&formatted);
            let continuation: Vec<_> = text
                .lines()
                .filter(|line| line.trim_start().starts_with('.'))
                .collect();
            assert!(!continuation.is_empty(), "{text}");
            assert!(
                continuation.iter().any(|line| line.contains(").")),
                "continuations must remain packed: {text}"
            );
            assert!(
                text.lines().all(|line| line.chars().count() <= width),
                "width {width}: {text}"
            );
        }
    }

    #[test]
    fn moving_multiline_methods_aligns_contents_without_changing_raw_strings() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sample.rs");
        let source = br###"#[component]
#[rustfmt::skip]
fn screen(cx: &mut UiContext) {
    column(cx).padding(20).children(|inner| {
        let raw = r#"literal
      keep these spaces
"#;

        row(inner).gap(8).children(|inner| {
            text(inner, "nested");
        });
    });
}"###;
        let mut formatter = Formatter::new(&path, "2024", 120).unwrap();
        let formatted = formatter.format(source).unwrap();
        assert_eq!(formatter.format(&formatted).unwrap(), formatted);
        let text = String::from_utf8_lossy(&formatted);
        assert!(
            text.contains("    column(cx).padding(20).children(|inner| {\n        let raw"),
            "{text}"
        );
        assert!(
            text.contains("            text(inner, \"nested\");"),
            "{text}"
        );
        assert!(text.contains("literal\n      keep these spaces\n"));
    }

    #[test]
    fn multiline_operator_headers_keep_nested_blocks_aligned() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sample.rs");
        let source = br#"#[component]
#[rustfmt::skip]
fn screen(cx: &mut UiContext) {
    column(cx).width_percent(100.).height(0.).grow(1.).align(Align::Center).scroll_y(true).padding(18).gap(16).background(PAPER).radius(10.) + |cx| {
        column(cx).width(320.).gap(14) + |cx| {
            text(cx, "content");
        };
    };
}"#;
        for width in [60, 80, 120] {
            let mut formatter = Formatter::new(&path, "2024", width).unwrap();
            let formatted = formatter.format(source).unwrap();
            let text = String::from_utf8_lossy(&formatted);
            assert!(text.contains("+ |cx|\n    {"), "{text}");
            assert!(
                text.contains("column(cx).width(320.).gap(14) + |cx| {"),
                "{text}"
            );
            assert_eq!(formatter.format(&formatted).unwrap(), formatted);
            assert_eq!(formatter.rustfmt(&formatted).unwrap(), formatted);
        }
    }

    #[test]
    fn invalid_source_and_nested_marked_functions_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sample.rs");
        let mut formatter = Formatter::new(&path, "2024", 120).unwrap();
        assert!(formatter.format(b"fn broken( {").is_err());
        let error = formatter
            .format(b"#[component] fn screen(cx: &mut UiContext) { column(cx);; {} }")
            .unwrap_err();
        assert!(error.to_string().contains("layout;; children was removed"));

        assert!(
            formatter
                .format(
                    br#"#[component] #[rustfmt::skip] fn outer(cx: &mut UiContext) {
            #[component] #[rustfmt::skip] fn inner(cx: &mut UiContext) {}
        }"#
                )
                .is_err()
        );
    }
}
