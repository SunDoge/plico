use crate::{Result, TreeSnafu};

/// Validate portable view data before activating an application module.
pub fn validate_nodes(nodes: &[plico_core::ViewNode]) -> Result<()> {
    fn walk(
        nodes: &[plico_core::ViewNode],
        depth: usize,
        count: &mut usize,
        in_button: bool,
    ) -> Result<()> {
        snafu::ensure!(
            depth < 128,
            TreeSnafu {
                message: "tree is too deep"
            }
        );
        let mut keys = std::collections::HashSet::new();
        for n in nodes {
            *count += 1;
            snafu::ensure!(
                *count <= 100_000,
                TreeSnafu {
                    message: "tree exceeds node limit"
                }
            );
            snafu::ensure!(
                keys.insert(&n.key),
                TreeSnafu {
                    message: "duplicate sibling key"
                }
            );
            snafu::ensure!(
                n.kind != plico_core::Kind::Root,
                TreeSnafu {
                    message: "nested synthetic root"
                }
            );
            snafu::ensure!(
                !(in_button && n.kind.interactive()),
                TreeSnafu {
                    message: "nested interactive control"
                }
            );
            snafu::ensure!(
                n.shader.as_ref().is_none_or(|p| p.valid())
                    && (n.kind == plico_core::Kind::Shader) == n.shader.is_some(),
                TreeSnafu {
                    message: "invalid shader parameters"
                }
            );
            snafu::ensure!(
                (n.kind == plico_core::Kind::Slider) == n.numeric.is_some()
                    && n.numeric.is_none_or(|range| range.valid()
                        && range.value >= range.min
                        && range.value <= range.max),
                TreeSnafu {
                    message: "invalid numeric payload"
                }
            );
            snafu::ensure!(
                (n.spans.is_empty() || n.kind == plico_core::Kind::Text)
                    && n.spans.iter().all(|span| span.valid_for(&n.text)),
                TreeSnafu {
                    message: "invalid rich text ranges"
                }
            );
            snafu::ensure!(
                n.grid.as_ref().is_none_or(|g| g.valid()),
                TreeSnafu {
                    message: "invalid grid template"
                }
            );
            let s = n.style;
            snafu::ensure!(
                s.valid(),
                TreeSnafu {
                    message: "invalid style"
                }
            );
            if matches!(
                n.kind,
                plico_core::Kind::Shader
                    | plico_core::Kind::Text
                    | plico_core::Kind::TextInput
                    | plico_core::Kind::Svg
                    | plico_core::Kind::Button
                    | plico_core::Kind::Switch
                    | plico_core::Kind::Checkbox
            ) {
                snafu::ensure!(
                    n.children.is_empty(),
                    TreeSnafu {
                        message: "leaf contains children"
                    }
                );
            }
            walk(
                &n.children,
                depth + 1,
                count,
                in_button || n.kind == plico_core::Kind::ButtonBase,
            )?;
        }
        Ok(())
    }
    walk(nodes, 0, &mut 0, false)
}
