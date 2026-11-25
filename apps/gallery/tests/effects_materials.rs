use gallery::{Gallery, Page};
use plico::Router;
use plico_desktop::testing::{Command, Driver};
#[test]
fn glass_profile_and_clarity_controls_update_the_gallery_without_enabling_motion() {
    let mut app = Gallery {
        router: Router::new(Page::Effects),
        ..Default::default()
    };
    app.effects.paused = true;
    let mut driver = Driver::new(app, 1120, 900).unwrap();
    for (label, value) in [
        ("Regular", "regular"),
        ("Clear", "clear"),
        ("Lens", "lens"),
        ("Custom", "custom"),
    ] {
        driver
            .execute(Command::Click {
                role: "button".into(),
                label: label.into(),
                occurrence: None,
            })
            .unwrap();
        assert_eq!(driver.view().effects.glass_style, value);
        assert!(driver.view().effects.paused);
    }
    driver
        .execute(Command::SetValue {
            label: "Pane clarity".into(),
            value: 0.8,
        })
        .unwrap();
    assert!((driver.view().effects.clarity - 0.8).abs() < 1e-6);
    driver
        .execute(Command::Click {
            role: "button".into(),
            label: "Unavailable".into(),
            occurrence: None,
        })
        .unwrap_err();
}

#[test]
fn preview_pointer_commands_capture_hover_press_and_release_without_changing_button_geometry() {
    use std::time::Duration;
    let mut app = Gallery {
        router: Router::new(Page::Effects),
        ..Default::default()
    };
    app.effects.paused = true;
    app.effects.glass_style = "regular".into();
    let mut driver = Driver::new(app, 1120, 900).unwrap();
    let bounds = driver
        .snapshot()
        .exposed_nodes()
        .iter()
        .find(|n| n.label.as_deref() == Some("Continue"))
        .unwrap()
        .bounds;
    let x = f64::from((bounds[0] + bounds[2]) * 0.5);
    let y = f64::from((bounds[1] + bounds[3]) * 0.5);
    driver.execute(Command::PointerMove { x, y }).unwrap();
    driver.advance(Duration::from_millis(120)).unwrap();
    driver.execute(Command::PointerDown { x, y }).unwrap();
    driver.advance(Duration::from_millis(150)).unwrap();
    assert!(driver.placed_nodes().iter().filter_map(|n|n.paint.widget.as_ref()).flat_map(|s|s.commands.iter()).any(|c|matches!(c,plico_renderer::PaintCommand::Shader {effect,..} if effect.values().len()>=20 && effect.values()[17]==1.)));
    let after = driver
        .snapshot()
        .exposed_nodes()
        .iter()
        .find(|n| n.label.as_deref() == Some("Continue"))
        .unwrap()
        .bounds;
    assert_eq!(bounds, after);
    driver.execute(Command::PointerUp { x, y }).unwrap();
    driver.advance(Duration::from_millis(150)).unwrap();
}

#[test]
fn liquid_controls_handle_native_actions_and_guard_slide_confirmation() {
    let app = Gallery {
        router: Router::new(Page::Effects),
        ..Default::default()
    };
    let mut driver = Driver::new(app, 1400, 2000).unwrap();
    let click = |driver: &mut Driver<Gallery>, role: &str, label: &str| {
        driver
            .execute(Command::Click {
                role: role.into(),
                label: label.into(),
                occurrence: None,
            })
            .unwrap();
    };
    click(&mut driver, "tab", "Details");
    assert_eq!(driver.view().effects.controls.tab, 1);
    click(&mut driver, "checkbox", "Show labels");
    assert!(driver.view().effects.controls.checked);
    click(&mut driver, "switch", "Highlight");
    assert!(driver.view().effects.controls.enabled);
    driver
        .execute(Command::SetValue {
            label: "Intensity".into(),
            value: 65.,
        })
        .unwrap();
    assert_eq!(driver.view().effects.controls.level, 65.);
    click(&mut driver, "button", "Apply");
    assert!(driver.view().effects.controls.applied);
    click(&mut driver, "button", "Reset action");
    assert!(!driver.view().effects.controls.applied);
    let bounds = driver
        .snapshot()
        .exposed_nodes()
        .iter()
        .find(|n| n.label.as_deref() == Some("Slide to confirm"))
        .unwrap()
        .bounds;
    let x = f64::from(bounds[0] + 12.);
    let y = f64::from((bounds[1] + bounds[3]) * 0.5);
    let end = f64::from(bounds[2] - 10.);
    for (start, stop, confirmed) in [(end, end, false), (x, x + 50., false), (x, end, true)] {
        driver
            .execute(Command::PointerDown { x: start, y })
            .unwrap();
        driver.execute(Command::PointerMove { x: stop, y }).unwrap();
        driver.execute(Command::PointerUp { x: stop, y }).unwrap();
        assert_eq!(driver.view().effects.controls.confirmed, confirmed);
    }
    click(&mut driver, "button", "Actions");
    driver
        .advance(std::time::Duration::from_millis(45))
        .unwrap();
    let mut halves = Vec::new();
    for node in driver.placed_nodes() {
        if let Some(widget) = &node.paint.widget {
            for command in &widget.commands {
                if let plico_renderer::PaintCommand::Shader { effect, .. } = command
                    && let Some(&side) = effect.values().get(39)
                    && side != 0.
                {
                    assert!(effect.values()[38] > 0.);
                    halves.push(side);
                }
            }
        }
    }
    halves.sort_by(f32::total_cmp);
    assert_eq!(halves, [-1., 1.]);
    assert_eq!(driver.shader_diagnostics().count(), 0);
    driver
        .advance(std::time::Duration::from_millis(800))
        .unwrap();
    click(&mut driver, "menuitem", "Reset intensity");
    assert_eq!(driver.view().effects.controls.level, 0.);
    click(&mut driver, "combobox", "Mode");
    click(&mut driver, "option", "Clear glass");
    assert_eq!(driver.view().effects.controls.mode, 1);
}

#[test]
fn showcase_dropdown_reopens_with_four_distinct_commands() {
    let mut driver = Driver::new(
        Gallery {
            router: Router::new(Page::LiquidGlass),
            ..Default::default()
        },
        1280,
        4400,
    )
    .unwrap();
    for chosen in ["Duplicate", "Share"] {
        driver
            .execute(Command::Click {
                role: "button".into(),
                label: "Options".into(),
                occurrence: None,
            })
            .unwrap();
        driver
            .advance(std::time::Duration::from_millis(550))
            .unwrap();
        for label in ["Rename", "Duplicate", "Share", "Delete"] {
            driver
                .get_by_role(plico_renderer::SemanticRole::MenuItem, label)
                .unwrap();
        }
        driver
            .execute(Command::Click {
                role: "menuitem".into(),
                label: chosen.into(),
                occurrence: None,
            })
            .unwrap();
        driver
            .advance(std::time::Duration::from_millis(550))
            .unwrap();
    }
}

#[test]
fn showcase_action_waits_for_completion_and_reset_cancels_the_previous_run() {
    use std::time::Duration;
    let mut driver = Driver::new(
        Gallery {
            router: Router::new(Page::LiquidGlass),
            ..Default::default()
        },
        1280,
        2000,
    )
    .unwrap();
    let click = |driver: &mut Driver<Gallery>, label: &str| {
        driver.execute(Command::Click {
            role: "button".into(),
            label: label.into(),
            occurrence: None,
        })
    };
    let visible = |driver: &Driver<Gallery>, label: &str| {
        driver
            .get_by_role(plico_renderer::SemanticRole::Button, label)
            .is_ok()
    };
    click(&mut driver, "Deploy to production").unwrap();
    driver.advance(Duration::from_millis(700)).unwrap();
    assert!(visible(&driver, "Deploying"));
    assert!(click(&mut driver, "Deploying").is_err());
    click(&mut driver, "Start over").unwrap();
    click(&mut driver, "Deploy to production").unwrap();
    driver.advance(Duration::from_millis(1399)).unwrap();
    assert!(visible(&driver, "Deploying"));
    driver.advance(Duration::from_millis(1)).unwrap();
    assert!(visible(&driver, "Deployed"));
    click(&mut driver, "Start over").unwrap();
    assert!(visible(&driver, "Deploy to production"));
}

#[test]
fn showcase_toggles_respond_to_pointer_on_indicator_and_label() {
    use plico_renderer::SemanticToggleState::{Off, On};
    let mut driver = Driver::new(
        Gallery {
            router: Router::new(Page::LiquidGlass),
            ..Default::default()
        },
        1280,
        4400,
    )
    .unwrap();
    for label in ["Notifications", "Remember this device", "Circle"] {
        let control = |driver: &Driver<Gallery>| {
            let snapshot = driver.snapshot();
            let node = snapshot
                .exposed_nodes()
                .into_iter()
                .find(|n| n.label.as_deref() == Some(label) && n.states.checked.is_some())
                .unwrap();
            (node.bounds, node.states.checked.unwrap())
        };
        let ([left, top, right, bottom], initial) = control(&driver);
        for (x, expected) in [
            (left + 8., if initial == On { Off } else { On }),
            (right - 8., initial),
        ] {
            let (x, y) = (f64::from(x), f64::from((top + bottom) * 0.5));
            driver.execute(Command::PointerDown { x, y }).unwrap();
            driver.execute(Command::PointerUp { x, y }).unwrap();
            assert_eq!(control(&driver).1, expected, "{label} at {x},{y}");
        }
        let role = if label == "Notifications" {
            "switch"
        } else {
            "checkbox"
        };
        driver
            .execute(Command::Focus {
                role: role.into(),
                label: label.into(),
                occurrence: None,
            })
            .unwrap();
        driver
            .execute(Command::Key {
                key: "Space".into(),
            })
            .unwrap();
        assert_ne!(control(&driver).1, initial, "{label} keyboard activation");
        if role == "switch" {
            let y = f64::from((top + bottom) * 0.5);
            let x = f64::from(left + 16.);
            driver.execute(Command::PointerDown { x, y }).unwrap();
            driver
                .execute(Command::PointerMove { x: x + 24., y })
                .unwrap();
            driver.execute(Command::PointerLeave).unwrap();
            assert_eq!(
                control(&driver).1,
                Off,
                "a cancelled drag leaves the switch off"
            );
            for (delta, expected) in [(5., Off), (24., On)] {
                driver.execute(Command::PointerDown { x, y }).unwrap();
                driver
                    .execute(Command::PointerMove { x: x + delta, y })
                    .unwrap();
                driver
                    .execute(Command::PointerUp { x: x + delta, y })
                    .unwrap();
                assert_eq!(control(&driver).1, expected, "switch dragged {delta}");
            }
        }
    }
}

#[test]
fn liquid_gallery_places_select_below_and_dropdown_over_its_anchor() {
    use plico_renderer::SemanticRole;
    let mut d = Driver::new(
        Gallery {
            router: Router::new(Page::LiquidGlass),
            ..Default::default()
        },
        1280,
        4400,
    )
    .unwrap();
    for (anchor_role, label, list_role, choice) in [
        (
            SemanticRole::ComboBox,
            "Delivery",
            SemanticRole::ListBox,
            "Express",
        ),
        (
            SemanticRole::Button,
            "Options",
            SemanticRole::Menu,
            "Duplicate",
        ),
    ] {
        let a = d.get_by_role(anchor_role, label).unwrap().bounds;
        let pointer_click = |d: &mut Driver<Gallery>, [left, top, right, bottom]: [f32; 4]| {
            let (x, y) = (
                f64::from((left + right) / 2.),
                f64::from((top + bottom) / 2.),
            );
            d.execute(Command::PointerDown { x, y }).unwrap();
            d.execute(Command::PointerUp { x, y }).unwrap();
        };
        pointer_click(&mut d, a);
        if list_role == SemanticRole::ListBox {
            d.advance(std::time::Duration::from_millis(96)).unwrap();
            let face = d
                .placed_nodes()
                .iter()
                .find(|node| {
                    node.paint.widget.as_ref().is_some_and(|widget| {
                        widget.commands.iter().any(|command| {
                            matches!(command,
                        plico_renderer::PaintCommand::Shader {effect,..}
                        if effect.values().get(13) == Some(&5.) && effect.values().get(22) != Some(&1.))
                        })
                    })
                })
                .expect("animated glass face");
            let [_, top, _, bottom] = face.own_clip.expect("expanded shader canvas");
            assert!(bottom - top - 2. * 28. < 90.);
            assert!(top + 28. <= a[3] + 12.);
        }

        for _ in 0..50 {
            d.advance(std::time::Duration::from_millis(16)).unwrap();
        }
        let panel = d.all_by_role(list_role)[0].bounds;
        if list_role == SemanticRole::ListBox {
            assert!(
                (panel[1] - a[3] - 12.).abs() < 1.,
                "panel={panel:?} anchor={a:?}"
            );
            assert!(
                d.get_by_role(SemanticRole::Option, "Pickup (closed today)")
                    .unwrap()
                    .disabled
            );
        } else {
            assert!((panel[0] + panel[2] - a[0] - a[2]).abs() < 2.);
            assert!((panel[1] + panel[3] - a[1] - a[3]).abs() < 2.);
        }
        let role = if list_role == SemanticRole::ListBox {
            SemanticRole::Option
        } else {
            SemanticRole::MenuItem
        };
        let row = d.get_by_role(role, choice).unwrap().bounds;
        assert!((row[3] - row[1] - 40.).abs() < 1.);
        pointer_click(&mut d, row);
        for _ in 0..50 {
            d.advance(std::time::Duration::from_millis(16)).unwrap();
        }
        assert!(d.all_by_role(list_role).is_empty());
    }
}

#[test]
fn centered_liquid_popups_throw_in_opposite_directions_and_close_softly() {
    use plico_renderer::{PaintCommand, SemanticRole::Button};
    use std::time::Duration;

    for (label, direction) in [("Options", 1.), ("Remind me", -1.), ("More", 1.)] {
        let mut driver = Driver::new(
            Gallery {
                router: Router::new(Page::LiquidGlass),
                ..Default::default()
            },
            1280,
            4400,
        )
        .unwrap();
        let anchor = driver.get_by_role(Button, label).unwrap().bounds;
        let surface = |driver: &Driver<Gallery>| {
            driver.placed_nodes().iter().find_map(|node| {
                node.paint
                    .widget
                    .as_ref()?
                    .commands
                    .iter()
                    .find_map(|command| {
                        if let PaintCommand::Shader { effect, .. } = command
                            && effect.values().get(22) == Some(&2.)
                        {
                            let [left, top, right, bottom] = node.own_clip?;
                            let outset = 44.;
                            return Some((
                                [left + outset, top + outset, right - outset, bottom - outset],
                                effect.values()[23],
                            ));
                        }
                        None
                    })
            })
        };
        driver
            .execute(Command::PointerClick {
                role: "button".into(),
                label: label.into(),
                occurrence: None,
            })
            .unwrap();

        driver.advance(Duration::from_millis(64)).unwrap();
        let (thrown, visible) = surface(&driver).unwrap();
        assert_eq!(visible, 0., "{label} should carry the trigger glass first");
        let height = |rect: [f32; 4]| rect[3] - rect[1];
        assert!((height(thrown) - height(anchor)).abs() < 1.);
        assert!(
            (thrown[1] - anchor[1]) * direction > 2.,
            "{label}: {thrown:?} vs {anchor:?}"
        );

        driver.advance(Duration::from_millis(144)).unwrap();
        let (expanding, visible) = surface(&driver).unwrap();
        assert_eq!(visible, 1.);
        assert!(height(expanding) > height(anchor) + 24.);
        assert!(
            driver
                .placed_nodes()
                .iter()
                .any(|node| node.paint.foreground_blur > 0.
                    && node.paint.runtime_transform.is_some())
        );

        driver.advance(Duration::from_millis(1150)).unwrap();
        let (open, _) = surface(&driver).unwrap();
        driver
            .execute(Command::Key {
                key: "Escape".into(),
            })
            .unwrap();
        driver.advance(Duration::from_millis(125)).unwrap();
        let (closing, _) = surface(&driver).unwrap();
        assert!(height(closing) < height(open) - 24.);
        assert!(height(closing) > height(anchor));
        driver.advance(Duration::from_millis(600)).unwrap();
        assert!(surface(&driver).is_none());
    }
}

#[test]
fn liquid_gallery_reminder_and_group_more_accept_pointer_choices() {
    let mut driver = Driver::new(
        Gallery {
            router: Router::new(Page::LiquidGlass),
            ..Default::default()
        },
        1280,
        4400,
    )
    .unwrap();
    let archive = driver
        .get_by_role(plico_renderer::SemanticRole::Button, "Archive")
        .unwrap()
        .bounds;
    let x = f64::from((archive[0] + archive[2]) * 0.5);
    let y = f64::from((archive[1] + archive[3]) * 0.5);
    driver.execute(Command::PointerMove { x, y }).unwrap();
    driver.execute(Command::PointerDown { x, y }).unwrap();
    driver
        .advance(std::time::Duration::from_millis(180))
        .unwrap();
    let pressed_glass: Vec<_> = driver
        .placed_nodes()
        .iter()
        .filter(|node| {
            node.paint.widget.as_ref().is_some_and(|widget| {
                widget.commands.iter().any(|command| {
                    matches!(command, plico_renderer::PaintCommand::Shader { effect, .. }
                if effect.values().get(23) == Some(&-1.) && effect.values().get(17) == Some(&1.))
                })
            })
        })
        .collect();
    assert_eq!(pressed_glass.len(), 1);
    assert!(pressed_glass[0].rect[2] - pressed_glass[0].rect[0] > archive[2] - archive[0] + 40.);
    driver.execute(Command::PointerUp { x, y }).unwrap();
    let click = |driver: &mut Driver<Gallery>, role: &str, label: &str| {
        driver
            .execute(Command::PointerClick {
                role: role.into(),
                label: label.into(),
                occurrence: None,
            })
            .unwrap();
        for _ in 0..52 {
            driver
                .advance(std::time::Duration::from_millis(16))
                .unwrap();
        }
    };
    click(&mut driver, "button", "More");
    click(&mut driver, "menuitem", "Mute thread");
    click(&mut driver, "button", "Remind me");
    click(&mut driver, "button", "Tonight at 8");
    assert!(
        driver
            .get_by_role(plico_renderer::SemanticRole::Button, "Tonight at 8")
            .is_ok()
    );
}

#[test]
fn showcase_tabs_drag_and_alarm_slider_commit_on_release() {
    use plico_renderer::SemanticRole::{Button, Tab};
    let mut driver = Driver::new(
        Gallery {
            router: Router::new(Page::LiquidGlass),
            ..Default::default()
        },
        1280,
        4400,
    )
    .unwrap();
    let center = |[left, top, right, bottom]: [f32; 4]| {
        (
            f64::from((left + right) * 0.5),
            f64::from((top + bottom) * 0.5),
        )
    };
    let (x, y) = center(driver.get_by_role(Tab, "Week").unwrap().bounds);
    let (end, _) = center(driver.get_by_role(Tab, "Year").unwrap().bounds);
    driver.execute(Command::PointerDown { x, y }).unwrap();
    driver.execute(Command::PointerMove { x: end, y }).unwrap();
    assert_eq!(
        driver.get_by_role(Tab, "Week").unwrap().states.selected,
        Some(true)
    );
    driver.execute(Command::PointerUp { x: end, y }).unwrap();
    assert_eq!(
        driver.get_by_role(Tab, "Year").unwrap().states.selected,
        Some(true)
    );

    let [left, top, right, bottom] = driver.get_by_role(Button, "Slide to stop").unwrap().bounds;
    let x = f64::from(left + 42.);
    let y = f64::from((top + bottom) * 0.5);
    for end in [x + 30., f64::from(right - 7.)] {
        driver.execute(Command::PointerDown { x, y }).unwrap();
        driver.execute(Command::PointerMove { x: end, y }).unwrap();
        driver.execute(Command::PointerUp { x: end, y }).unwrap();
        assert_eq!(
            driver
                .get_by_role(Button, "Slide to stop")
                .unwrap()
                .disabled,
            end > x + 30.
        );
    }
    driver
        .advance(std::time::Duration::from_millis(2600))
        .unwrap();
    assert!(
        !driver
            .get_by_role(Button, "Slide to stop")
            .unwrap()
            .disabled
    );
}

#[test]
fn showcase_brightness_preserves_grab_offset_and_maps_the_full_track() {
    use plico_renderer::SemanticRole::Slider;
    let mut driver = Driver::new(
        Gallery {
            router: Router::new(Page::LiquidGlass),
            ..Default::default()
        },
        1280,
        4400,
    )
    .unwrap();
    driver
        .advance(std::time::Duration::from_millis(500))
        .unwrap();
    let brightness = |driver: &Driver<Gallery>| driver.get_by_role(Slider, "Brightness").unwrap();
    let [left, top, right, bottom] = brightness(&driver).bounds;
    let width = f64::from(right - left);
    let y = f64::from((top + bottom) * 0.5);
    let grab = f64::from(left) + width * 0.4 + 14.;
    driver.execute(Command::PointerDown { x: grab, y }).unwrap();
    assert_eq!(brightness(&driver).numeric_value, Some(40.));
    driver
        .execute(Command::PointerMove {
            x: grab + width * 0.25,
            y,
        })
        .unwrap();
    assert_eq!(brightness(&driver).numeric_value, Some(65.));
    driver
        .execute(Command::PointerUp {
            x: grab + width * 0.25,
            y,
        })
        .unwrap();
    let [left, _, right, _] = brightness(&driver).bounds;
    let x = f64::from(left + (right - left) * 0.05);
    driver.execute(Command::PointerDown { x, y }).unwrap();
    driver.execute(Command::PointerUp { x, y }).unwrap();
    assert_eq!(brightness(&driver).numeric_value, Some(5.));
    driver
        .execute(Command::Key {
            key: "ArrowRight".into(),
        })
        .unwrap();
    assert_eq!(brightness(&driver).numeric_value, Some(6.));
}

#[test]
fn stretched_dropdown_reveals_items_in_order_and_retracts() {
    use plico_renderer::{PaintCommand, SemanticRole};
    use std::time::Duration;

    let mut driver = Driver::new(
        Gallery {
            router: Router::new(Page::LiquidGlass),
            ..Default::default()
        },
        1280,
        4400,
    )
    .unwrap();
    let trigger = driver
        .get_by_role(SemanticRole::Button, "Actions")
        .unwrap()
        .bounds;
    driver
        .execute(Command::PointerClick {
            role: "button".into(),
            label: "Actions".into(),
            occurrence: None,
        })
        .unwrap();
    driver.advance(Duration::from_millis(110)).unwrap();
    let face_height = |driver: &Driver<Gallery>| {
        driver
            .placed_nodes()
            .iter()
            .find_map(|node| {
                node.paint
                    .widget
                    .as_ref()?
                    .commands
                    .iter()
                    .find_map(|command| {
                        if let PaintCommand::Shader { effect, .. } = command
                            && effect.values().get(13) == Some(&5.)
                            && effect.values().get(22) == Some(&0.)
                        {
                            return Some(effect.logical_size()[1]);
                        }
                        None
                    })
            })
            .unwrap()
    };
    let early_height = face_height(&driver);
    let opacity = |driver: &Driver<Gallery>, label: &str| {
        driver
            .placed_nodes()
            .iter()
            .find(|node| node.paint.text.as_deref() == Some(label))
            .unwrap()
            .world_opacity
    };
    assert_eq!(opacity(&driver, "Rename"), 0.);
    driver.advance(Duration::from_millis(265)).unwrap();
    assert!(opacity(&driver, "Rename") > opacity(&driver, "Duplicate"));
    assert!(opacity(&driver, "Duplicate") > opacity(&driver, "Share"));

    driver.advance(Duration::from_millis(525)).unwrap();
    assert_eq!(opacity(&driver, "Delete"), 1.);
    let open_height = face_height(&driver);
    assert!(open_height > early_height + 40.);
    let panel = driver
        .get_by_role(SemanticRole::Menu, "Actions")
        .unwrap()
        .bounds;
    assert!((panel[1] - trigger[3] - 12.).abs() < 1.);
    assert!((panel[0] + panel[2] - trigger[0] - trigger[2]).abs() < 2.);

    driver
        .execute(Command::Key {
            key: "Escape".into(),
        })
        .unwrap();
    driver.advance(Duration::from_millis(160)).unwrap();
    assert!(opacity(&driver, "Rename") > opacity(&driver, "Delete"));
    driver.advance(Duration::from_millis(250)).unwrap();
    assert!(driver.all_by_role(SemanticRole::Menu).is_empty());
}

#[test]
fn liquid_showcase_reflows_live_controls_and_records_the_overview() {
    use plico_desktop::PreviewScenario;
    use plico_renderer::SemanticRole::{Button, Label, TreeItem};

    let scenario: PreviewScenario<Gallery> =
        serde_json::from_str(include_str!("../scenarios/liquid-showcase-recording.json")).unwrap();
    let mut driver = Driver::new(Gallery::default(), 1920, 1080).unwrap();
    assert!(driver.get_by_role(TreeItem, "Liquid Glass").is_ok());
    driver
        .execute(Command::Click {
            role: "treeitem".into(),
            label: "Liquid Glass Showcase".into(),
            occurrence: None,
        })
        .unwrap();
    assert_eq!(driver.view().router.current(), Page::LiquidGlassShowcase);

    let button = driver.get_by_role(Button, "Play").unwrap().id;
    let bounds = |driver: &Driver<Gallery>, title| driver.get_by_role(Label, title).unwrap().bounds;
    let first = bounds(&driver, "Button");
    let second = bounds(&driver, "Action Button");
    assert_eq!(first[1], second[1]);
    assert!(second[0] > first[2]);
    assert!(bounds(&driver, "Popover")[3] < 1080.);

    driver.resize(640, 740).unwrap();
    let first = bounds(&driver, "Button");
    let second = bounds(&driver, "Action Button");
    assert_eq!(first[0], second[0]);
    assert!(second[1] > first[3]);
    assert_eq!(driver.get_by_role(Button, "Play").unwrap().id, button);

    driver.resize(1920, 1080).unwrap();
    for step in scenario.steps {
        // Apply the same fixed-rate samples that drive popup hit targets in a recording.
        let mut remaining = std::time::Duration::from_millis(step.after_ms);
        while !remaining.is_zero() {
            let tick = remaining.min(std::time::Duration::from_millis(16));
            driver.advance(tick).unwrap();
            remaining -= tick;
        }
        driver.execute(step.command).unwrap();
    }
    assert!(driver.get_by_role(Button, "Pause").is_ok());
    assert!(driver.get_by_role(Button, "Tonight at 8").is_ok());
}

#[test]
fn liquid_showcase_autoplay_loops_and_preserves_manual_controls() {
    use plico_renderer::{
        SemanticRole::{Button, CheckBox, Slider, Tab},
        SemanticToggleState::{Off, On},
        UiEvent, WindowMetrics,
    };
    use std::time::Duration;

    let mut driver = Driver::new(
        Gallery {
            router: Router::new(Page::LiquidGlassShowcase),
            ..Default::default()
        },
        1920,
        1080,
    )
    .unwrap();
    let checkbox = "Autoplay animations";
    let toggle = || Command::PointerClick {
        role: "checkbox".into(),
        label: checkbox.into(),
        occurrence: None,
    };
    driver
        .execute(Command::SetValue {
            label: "Brightness".into(),
            value: 65.,
        })
        .unwrap();
    driver.advance(Duration::from_secs(8)).unwrap();
    assert_eq!(
        driver
            .get_by_role(CheckBox, checkbox)
            .unwrap()
            .states
            .checked,
        Some(Off)
    );
    assert!(driver.get_by_role(Button, "Play").is_ok());

    driver.execute(toggle()).unwrap();
    driver.advance(Duration::from_millis(300)).unwrap();
    assert!(driver.get_by_role(Button, "Deploying").is_ok());
    assert_eq!(
        driver.get_by_role(Tab, "Year").unwrap().states.selected,
        Some(true)
    );
    assert_eq!(
        driver
            .get_by_role(Slider, "Brightness")
            .unwrap()
            .numeric_value,
        Some(82.)
    );
    let rows = ["Same day", "Mute thread", "Duplicate", "Tonight at 8"];
    for row in rows {
        assert!(
            showcase_paints(&driver, row),
            "{row} should open in the same frame"
        );
    }
    driver.advance(Duration::from_millis(1400)).unwrap();
    assert!(driver.get_by_role(Button, "Deployed").is_ok());
    driver.advance(Duration::from_millis(1400)).unwrap();
    assert!(driver.get_by_role(Button, "Play").is_ok());
    driver.advance(Duration::from_millis(750)).unwrap();
    for row in rows {
        assert!(!showcase_paints(&driver, row), "{row} should be closed");
    }
    driver.advance(Duration::from_millis(200)).unwrap();
    assert!(driver.get_by_role(Button, "Deploying").is_ok());
    for row in rows {
        assert!(
            showcase_paints(&driver, row),
            "{row} should reopen in the next cycle"
        );
    }
    assert_eq!(
        driver
            .get_by_role(CheckBox, checkbox)
            .unwrap()
            .states
            .checked,
        Some(On)
    );

    driver.execute(toggle()).unwrap();
    driver.advance(Duration::from_secs(8)).unwrap();
    assert!(driver.get_by_role(Button, "Play").is_ok());
    assert_eq!(
        driver
            .get_by_role(Slider, "Brightness")
            .unwrap()
            .numeric_value,
        Some(65.)
    );

    driver.execute(toggle()).unwrap();
    driver.advance(Duration::from_millis(300)).unwrap();
    driver
        .dispatch_event(UiEvent::WindowMetrics(WindowMetrics {
            reduced_motion: true,
            ..Default::default()
        }))
        .unwrap();
    driver.advance(Duration::from_secs(8)).unwrap();
    let control = driver.get_by_role(CheckBox, checkbox).unwrap();
    assert!(control.disabled);
    assert_eq!(control.states.checked, Some(Off));
    assert!(driver.get_by_role(Button, "Play").is_ok());
}

fn showcase_paints(driver: &Driver<Gallery>, label: &str) -> bool {
    driver
        .placed_nodes()
        .iter()
        .any(|node| node.paint.text.as_deref() == Some(label))
}

#[test]
fn liquid_showcase_autoplay_popup_keeps_focus_and_allows_pointer_input() {
    use plico_renderer::{
        SemanticRole::{CheckBox, ComboBox, Option},
        SemanticToggleState::Off,
    };
    use std::time::Duration;

    for (width, height) in [(1920, 1080), (1120, 820)] {
        let mut driver = Driver::new(
            Gallery {
                router: Router::new(Page::LiquidGlassShowcase),
                ..Default::default()
            },
            width,
            height,
        )
        .unwrap();
        let label = "Autoplay animations";
        let click = || Command::PointerClick {
            role: "checkbox".into(),
            label: label.into(),
            occurrence: None,
        };
        let advance_to_delivery = |driver: &mut Driver<Gallery>| {
            driver.advance(Duration::from_millis(300)).unwrap();
        };
        driver.execute(click()).unwrap();
        driver
            .execute(Command::Focus {
                role: "slider".into(),
                label: "Brightness".into(),
                occurrence: None,
            })
            .unwrap();
        let focus = driver.snapshot().focus.expect("slider focused");
        advance_to_delivery(&mut driver);
        assert_eq!(showcase_paints(&driver, "Same day"), width == 1920);
        assert_eq!(driver.snapshot().focus, Some(focus));
        assert!(driver.get_by_role(Option, "Same day").is_err());
        let expanded = driver
            .get_by_role(ComboBox, "Delivery")
            .unwrap()
            .states
            .expanded;
        assert_eq!(expanded, Some(false));
        driver.execute(click()).unwrap();
        assert_eq!(
            driver.get_by_role(CheckBox, label).unwrap().states.checked,
            Some(Off)
        );
        driver.advance(Duration::from_secs(12)).unwrap();
        assert_eq!(
            driver
                .get_by_role(ComboBox, "Delivery")
                .unwrap()
                .states
                .expanded,
            Some(false)
        );

        if width == 1920 {
            driver.execute(click()).unwrap();
            advance_to_delivery(&mut driver);
            driver
                .execute(Command::PointerClick {
                    role: "combobox".into(),
                    label: "Delivery".into(),
                    occurrence: None,
                })
                .unwrap();
            driver.advance(Duration::from_millis(2800)).unwrap();
            assert_eq!(
                driver
                    .get_by_role(ComboBox, "Delivery")
                    .unwrap()
                    .states
                    .expanded,
                Some(true)
            );
            assert!(driver.get_by_role(Option, "Same day").is_ok());
        }
    }
}

#[test]
fn clear_button_uses_the_same_press_spring_for_pointer_and_autoplay() {
    use plico_renderer::{PaintCommand, SemanticRole::Button};
    use std::time::Duration;

    let make_driver = |page| {
        Driver::new(
            Gallery {
                router: Router::new(page),
                ..Default::default()
            },
            1920,
            1080,
        )
        .unwrap()
    };
    let optics = |driver: &Driver<Gallery>| {
        driver
            .placed_nodes()
            .iter()
            .filter_map(|node| node.paint.widget.as_ref())
            .flat_map(|widget| &widget.commands)
            .find_map(|command| match command {
                PaintCommand::Shader { effect, .. }
                    if effect.values().get(13) == Some(&6.)
                        && effect.values().get(23) == Some(&0.) =>
                {
                    let values = effect.values();
                    Some([values[17], values[25], values[26]])
                }
                _ => None,
            })
            .expect("clear button exposes the press and its horizontal and vertical growth")
    };
    let mut pointer = make_driver(Page::LiquidGlass);
    let [left, top, right, bottom] = pointer.get_by_role(Button, "Play").unwrap().bounds;
    let (x, y) = (
        f64::from((left + right) / 2.),
        f64::from((top + bottom) / 2.),
    );
    assert_eq!(optics(&pointer), [0.; 3]);
    pointer.execute(Command::PointerMove { x, y }).unwrap();
    pointer.advance(Duration::from_millis(100)).unwrap();
    let [press, horizontal, vertical] = optics(&pointer);
    assert_eq!(press, 0.);
    assert!(horizontal > vertical && vertical > 0.);
    pointer.advance(Duration::from_millis(100)).unwrap();
    assert!(
        optics(&pointer)[1] > 5.12,
        "hover should overshoot its rest size"
    );

    pointer.execute(Command::PointerDown { x, y }).unwrap();
    pointer.advance(Duration::from_millis(100)).unwrap();
    let manual_press = optics(&pointer)[0];
    assert!(manual_press > 0.5);
    assert_eq!(
        pointer.get_by_role(Button, "Play").unwrap().bounds,
        [left, top, right, bottom],
        "optical growth must not reflow the hit bounds"
    );
    pointer.execute(Command::PointerUp { x, y }).unwrap();
    assert!(pointer.get_by_role(Button, "Pause").is_ok());
    pointer.execute(Command::PointerLeave).unwrap();
    pointer.advance(Duration::from_secs(2)).unwrap();
    assert!(optics(&pointer).into_iter().all(|value| value.abs() < 0.01));

    let mut autoplay = make_driver(Page::LiquidGlassShowcase);
    autoplay
        .execute(Command::PointerClick {
            role: "checkbox".into(),
            label: "Autoplay animations".into(),
            occurrence: None,
        })
        .unwrap();
    autoplay.advance(Duration::from_millis(300)).unwrap();
    autoplay.advance(Duration::from_millis(100)).unwrap();
    assert!(
        (optics(&autoplay)[0] - manual_press).abs() < 0.001,
        "visual preview should follow the native pointer press curve"
    );
    let label = autoplay
        .placed_nodes()
        .iter()
        .find(|node| node.paint.text.as_deref() == Some("Play"))
        .unwrap();
    assert!(label.world_transform.as_coeffs()[0] > 1.);
    assert!(!label.paint.wrap_text && !label.paint.text_ellipsis);
    assert!(autoplay.get_by_role(Button, "Play").is_ok());
    autoplay.advance(Duration::from_millis(60)).unwrap();
    assert!(autoplay.get_by_role(Button, "Pause").is_ok());
    assert!(
        optics(&autoplay)[0] > 0.5,
        "release starts from the held pose"
    );
}
