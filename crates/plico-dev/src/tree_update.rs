//! Transfer plain text edits against the last acknowledged portable tree.
use plico::{Kind, ViewNode};
use rkyv::{Archive, Deserialize, Serialize};

#[derive(Archive, Serialize, Deserialize)]
pub enum TreeUpdate {
    Full(Vec<ViewNode>),
    Text(TextPatch),
}

#[derive(Archive, Serialize, Deserialize)]
pub struct TextPatch(Vec<(usize, String)>);

fn editable(node: &ViewNode) -> bool {
    matches!(node.kind, Kind::Text | Kind::Button | Kind::ButtonBase) && node.spans.is_empty()
}

impl TextPatch {
    /// Indices refer to preorder positions in the acknowledged tree. All other
    /// node properties must be identical for a text update to be valid.
    pub fn between(old: &[ViewNode], new: &[ViewNode]) -> Option<Self> {
        fn compare(
            old: &[ViewNode],
            new: &[ViewNode],
            index: &mut usize,
            edits: &mut Vec<(usize, String)>,
        ) -> bool {
            old.len() == new.len()
                && old.iter().zip(new).all(|(a, b)| {
                    let ViewNode {
                        key,
                        kind,
                        style,
                        text,
                        spans,
                        grid,
                        shader,
                        numeric,
                        children,
                    } = a;
                    if key != &b.key
                        || kind != &b.kind
                        || style != &b.style
                        || spans != &b.spans
                        || grid != &b.grid
                        || shader != &b.shader
                        || numeric != &b.numeric
                    {
                        return false;
                    }
                    let position = *index;
                    *index += 1;
                    if text != &b.text {
                        if !editable(b) {
                            return false;
                        }
                        edits.push((position, b.text.clone()));
                    }
                    compare(children, &b.children, index, edits)
                })
        }
        let mut edits = Vec::new();
        compare(old, new, &mut 0, &mut edits).then_some(Self(edits))
    }

    /// Reject malformed edits before modifying any previously displayed node.
    pub fn apply(self, previous: &mut [ViewNode]) -> crate::Result<()> {
        let Self(edits) = self;
        fn collect<'a>(
            nodes: &'a mut [ViewNode],
            index: &mut usize,
            edits: &mut &[(usize, String)],
            targets: &mut Vec<&'a mut String>,
        ) {
            for node in nodes {
                let Some((position, _)) = edits.first() else {
                    break;
                };
                if *position == *index && editable(node) {
                    targets.push(&mut node.text);
                    *edits = &edits[1..];
                }
                *index += 1;
                collect(&mut node.children, index, edits, targets);
            }
        }
        let mut rest = edits.as_slice();
        let mut targets = Vec::with_capacity(edits.len());
        collect(previous, &mut 0, &mut rest, &mut targets);
        if !rest.is_empty() {
            return Err(crate::Error::Tree {
                message: "invalid text edit target or order".into(),
            });
        }
        for (target, (_, value)) in targets.into_iter().zip(edits) {
            *target = value;
        }
        Ok(())
    }
}

impl TreeUpdate {
    pub fn resolve(self, previous: &mut Vec<ViewNode>) -> crate::Result<Vec<ViewNode>> {
        match self {
            Self::Full(nodes) => {
                crate::validate_nodes(&nodes)?;
                Ok(nodes)
            }
            Self::Text(patch) => {
                patch.apply(previous)?;
                Ok(std::mem::take(previous))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plico::prelude::*;

    #[test]
    fn applying_text_edits_is_atomic_and_structural_changes_require_a_full_tree() {
        let mut engine = plico::Engine::default();
        engine
            .frame_declarations(|cx| {
                column(cx).children(|cx| {
                    text(cx, "old");
                    text(cx, "second");
                });
            })
            .unwrap();
        let mut original = engine.snapshot();
        let mut updated = original.clone();
        updated[0].children[0].text = "new".into();
        let patch = TextPatch::between(&original, &updated).unwrap();
        for invalid in [0, 1, 99] {
            assert!(
                TextPatch(vec![(1, "partial".into()), (invalid, "invalid".into())])
                    .apply(&mut original)
                    .is_err()
            );
            assert_eq!(original[0].children[0].text, "old");
        }
        patch.apply(&mut original).unwrap();
        assert_eq!(original, updated);
        updated[0].children.swap(0, 1);
        assert!(TextPatch::between(&original, &updated).is_none());
    }
}
