use ncview_rs::{
    app::{
        AppState, Command, Effect, Generation, GridMode, InputMode, LimitField, Overlay,
        PlotAxisField, PlotKind, PlotSeries, PlotYAxis,
    },
    data::{
        Variable,
        slice::{Bounds, Slice2D, Validity},
    },
    ui::sidebar::filter_variables,
};
use ndarray::{Array2, arr2};

#[test]
fn superseded_map_and_plot_results_cannot_replace_last_valid_results() {
    let mut state = AppState::default();
    let previous = Slice2D::new(
        arr2(&[[1.0, 2.0], [3.0, 4.0]]),
        Array2::from_elem((2, 2), Validity::Finite),
        Bounds::new(0, 2, 0, 2).unwrap(),
    )
    .unwrap();
    state.set_slice(previous.clone());
    assert_eq!(state.view.decoded_bytes, previous.memory_bytes());
    state.view.generation = Generation(4);
    state.view.plot_generation = Generation(7);
    state.view.plot_series = vec![PlotSeries {
        point: (0, 0),
        label: "previous".into(),
        data: vec![(0.0, 1.0)],
        labels: vec!["t0".into()],
    }];

    assert!(!state.accept_slice(Generation(3), previous));
    assert_eq!(state.view.slice.as_ref().unwrap().values[[0, 0]], 1.0);
    assert!(!state.accept_plot(
        Generation(6),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        "stale".into(),
    ));
    assert_eq!(state.view.plot_series[0].label, "previous");

    let next = state.next_generation();
    assert_eq!(next, Generation(5));
    let next_plot = state.next_plot_generation();
    assert_eq!(next_plot, Generation(8));
}

#[test]
fn variable_selection_advances_generation_and_emits_read_effect() {
    let mut state = AppState {
        variables: vec![Variable {
            name: "temperature".into(),
            dimensions: vec!["lat".into(), "lon".into()],
            numeric: true,
            units: None,
            long_name: None,
            standard_name: None,
        }],
        ..AppState::default()
    };
    state.view.limits = Some((1.0, 2.0));
    state.view.limits_manual = true;
    let effect = state.reduce(Command::SelectVariable(0));
    assert!(matches!(effect, Some(Effect::ReadSlice { .. })));
    assert_eq!(state.view.generation.0, 1);
    assert_eq!(state.view.selected_variable.as_deref(), Some("temperature"));
    assert_eq!(state.view.limits, None);
    assert!(!state.view.limits_manual);
}

#[test]
fn variable_selection_moves_between_plottable_fields() {
    let mut state = AppState {
        variables: vec![
            Variable {
                name: "Pixel_area".into(),
                dimensions: vec!["date".into(), "lat".into(), "lon".into()],
                numeric: true,
                units: None,
                long_name: None,
                standard_name: None,
            },
            Variable {
                name: "MACCity".into(),
                dimensions: vec!["date".into(), "lat".into(), "lon".into()],
                numeric: true,
                units: None,
                long_name: None,
                standard_name: None,
            },
        ],
        ..AppState::default()
    };
    state.view.selected_variable = Some("MACCity".into());
    state.reduce(Command::SelectVariable(0));
    assert_eq!(state.view.selected_variable.as_deref(), Some("Pixel_area"));
}

#[test]
fn variable_browser_moves_without_loading_until_submit() {
    let mut state = AppState {
        variables: vec![
            Variable {
                name: "temperature".into(),
                dimensions: vec!["lat".into(), "lon".into()],
                numeric: true,
                units: None,
                long_name: None,
                standard_name: None,
            },
            Variable {
                name: "humidity".into(),
                dimensions: vec!["lat".into(), "lon".into()],
                numeric: true,
                units: None,
                long_name: None,
                standard_name: None,
            },
        ],
        ..AppState::default()
    };
    state.view.selected_variable = Some("temperature".into());
    state.reduce(Command::OpenVariableSearch);

    assert!(state.reduce(Command::SelectVariable(1)).is_none());
    assert_eq!(state.view.selected_variable.as_deref(), Some("temperature"));
    assert_eq!(state.view.variable_browser_index, 1);

    let effect = state.reduce(Command::SubmitVariableSearch);
    assert!(matches!(effect, Some(Effect::ReadSlice { .. })));
    assert_eq!(state.view.selected_variable.as_deref(), Some("humidity"));
    assert!(!state.view.variable_search_active);
}

#[test]
fn navigation_is_clamped_and_filter_is_fuzzy() {
    let mut state = AppState::default();
    state.view.time_length = 3;
    state.view.depth_length = 2;
    state.reduce(Command::MoveTime(99));
    state.reduce(Command::MoveDepth(-1));
    assert_eq!(state.view.time_index, 2);
    assert_eq!(state.view.depth_index, 0);

    let variables = vec![
        Variable {
            name: "sea_surface_temperature".into(),
            dimensions: vec![],
            numeric: true,
            units: None,
            long_name: None,
            standard_name: None,
        },
        Variable {
            name: "salinity".into(),
            dimensions: vec![],
            numeric: true,
            units: None,
            long_name: None,
            standard_name: None,
        },
    ];
    assert_eq!(filter_variables(&variables, "sst").len(), 1);
}

#[test]
fn grid_mode_toggle_changes_projection_generation_state() {
    let mut state = AppState::default();
    assert_eq!(state.view.grid_mode, GridMode::Logical);
    state.reduce(Command::ToggleGridMode);
    assert_eq!(state.view.grid_mode, GridMode::Projected);
}

#[test]
fn limits_dialog_accepts_manual_minimum_and_maximum() {
    let mut state = AppState::default();
    state.reduce(Command::OpenLimits);
    state.reduce(Command::InputChar('2'));
    state.reduce(Command::NextLimitField);
    state.reduce(Command::InputChar('8'));
    state.reduce(Command::ActivatePoint);
    assert_eq!(state.view.limits, Some((2.0, 8.0)));
    assert!(state.view.overlay.is_none());
}

#[test]
fn limit_field_can_be_focused_explicitly() {
    let mut state = AppState::default();
    state.reduce(Command::OpenLimits);
    state.reduce(Command::FocusLimitField(LimitField::Max));
    state.reduce(Command::InputChar('9'));
    assert_eq!(
        state
            .view
            .limit_draft
            .as_ref()
            .map(|draft| draft.max.as_str()),
        Some("9")
    );
}

#[test]
fn command_palette_filters_and_executes_actions() {
    let mut state = AppState::default();
    state.reduce(Command::OpenCommandPalette);
    assert_eq!(state.view.overlay, Some(Overlay::CommandPalette));
    for character in "colormap".chars() {
        state.reduce(Command::InputChar(character));
    }
    state.reduce(Command::ActivatePoint);
    assert_eq!(
        state.view.palette,
        ncview_rs::render::colors::Palette::Plasma
    );
    assert!(state.view.overlay.is_none());
}

#[test]
fn map_hover_and_click_pin_a_point_for_time_series() {
    let mut state = AppState::default();
    state.reduce(Command::HoverPoint {
        x: 12,
        y: 8,
        row: 4,
        col: 7,
        value: Some(273.15),
    });
    assert_eq!(
        state.view.hover_point.as_ref().map(|point| point.row),
        Some(4)
    );
    state.reduce(Command::SelectPoint { row: 4, col: 7 });
    assert_eq!(state.view.selected_point, Some((4, 7)));
    state.reduce(Command::ActivatePoint);
    assert_eq!(state.view.overlay, Some(Overlay::Plot));
}

#[test]
fn point_plot_chooser_switches_kind_and_histogram_axis() {
    let mut state = AppState::default();
    state.reduce(Command::SelectPoint { row: 2, col: 3 });
    state.reduce(Command::OpenPlot);
    assert_eq!(state.view.overlay, Some(Overlay::Plot));
    assert_eq!(state.view.plot_draft.kind, PlotKind::TimeSeries);

    state.reduce(Command::SetPlotKind(PlotKind::Histogram));
    assert_eq!(state.view.plot_draft.y_axis, PlotYAxis::Frequency);
    state.reduce(Command::FocusPlotAxis(PlotAxisField::Y));
    state.reduce(Command::CyclePlotAxis(1));
    assert_eq!(state.view.plot_draft.y_axis, PlotYAxis::Density);
}

#[test]
fn point_selection_can_accumulate_and_remove_multiple_points() {
    let mut state = AppState::default();
    state.reduce(Command::HoverPoint {
        x: 1,
        y: 1,
        row: 2,
        col: 3,
        value: Some(1.0),
    });
    state.reduce(Command::TogglePointSelection);
    state.reduce(Command::HoverPoint {
        x: 2,
        y: 2,
        row: 5,
        col: 7,
        value: Some(2.0),
    });
    state.reduce(Command::TogglePointSelection);
    assert_eq!(state.view.selected_points, vec![(2, 3), (5, 7)]);
    assert_eq!(state.view.selected_point, Some((5, 7)));

    state.reduce(Command::TogglePointSelection);
    assert_eq!(state.view.selected_points, vec![(2, 3)]);
    assert_eq!(state.view.selected_point, Some((2, 3)));
}

#[test]
fn scatter_plot_uses_time_axis_and_can_cycle_to_sample_index() {
    let mut state = AppState::default();
    state.reduce(Command::SelectPoint { row: 2, col: 3 });
    state.reduce(Command::OpenPlot);
    state.reduce(Command::SetPlotKind(PlotKind::Scatter));
    assert_eq!(
        state.view.plot_draft.x_axis,
        ncview_rs::app::PlotXAxis::ValidTime
    );
    state.reduce(Command::CyclePlotAxis(1));
    assert_eq!(
        state.view.plot_draft.x_axis,
        ncview_rs::app::PlotXAxis::SampleIndex
    );
    state.reduce(Command::CyclePlotAxis(1));
    assert_eq!(
        state.view.plot_draft.x_axis,
        ncview_rs::app::PlotXAxis::Longitude
    );
    state.reduce(Command::CyclePlotAxis(1));
    assert_eq!(
        state.view.plot_draft.x_axis,
        ncview_rs::app::PlotXAxis::Latitude
    );
}

#[test]
fn plot_axis_cycle_includes_arbitrary_variable_dimensions() {
    let mut state = AppState::default();
    state.view.axis_options = vec![
        "time".into(),
        "isobaricInhPa".into(),
        "ensemble".into(),
        "latitude".into(),
        "longitude".into(),
    ];
    state.reduce(Command::SelectPoint { row: 1, col: 2 });
    state.reduce(Command::OpenPlot);

    state.reduce(Command::CyclePlotAxis(1));
    state.reduce(Command::CyclePlotAxis(1));
    assert_eq!(
        state.view.plot_draft.x_axis,
        ncview_rs::app::PlotXAxis::Dimension(1)
    );
    state.reduce(Command::CyclePlotAxis(1));
    assert_eq!(
        state.view.plot_draft.x_axis,
        ncview_rs::app::PlotXAxis::Dimension(2)
    );
}

#[test]
fn variable_search_filters_and_submits_the_first_match() {
    let mut state = AppState {
        variables: vec![
            Variable {
                name: "temperature_surface".into(),
                dimensions: vec!["lat".into(), "lon".into()],
                numeric: true,
                units: None,
                long_name: None,
                standard_name: None,
            },
            Variable {
                name: "salinity".into(),
                dimensions: vec!["lat".into(), "lon".into()],
                numeric: true,
                units: None,
                long_name: None,
                standard_name: None,
            },
        ],
        ..AppState::default()
    };
    state.reduce(Command::OpenVariableSearch);
    state.reduce(Command::InputChar('s'));
    state.reduce(Command::InputChar('a'));
    state.reduce(Command::InputChar('l'));
    let effect = state.reduce(Command::SubmitVariableSearch);
    assert!(matches!(effect, Some(Effect::ReadSlice { .. })));
    assert_eq!(state.view.selected_variable.as_deref(), Some("salinity"));
    assert!(!state.view.variable_search_active);
}

#[test]
fn axis_choices_cycle_and_pan_stays_inside_the_full_bounds() {
    let mut state = AppState::default();
    state.view.axis_options = vec!["date".into(), "lat".into(), "lon".into()];
    state.reduce(Command::OpenAxisOverlay);
    state.reduce(Command::CycleAxis(-1));
    assert_ne!(
        state.view.axis_draft.as_ref().unwrap().x,
        state.view.axis_draft.as_ref().unwrap().y
    );
    state.view.full_bounds = Some(ncview_rs::data::slice::Bounds::new(0, 10, 0, 20).unwrap());
    state.view.zoom_bounds = Some(ncview_rs::data::slice::Bounds::new(2, 6, 4, 10).unwrap());
    state.reduce(Command::Pan { rows: 99, cols: 99 });
    assert_eq!(
        state.view.zoom_bounds,
        Some(ncview_rs::data::slice::Bounds::new(6, 10, 14, 20).unwrap())
    );
}

#[test]
fn input_mode_reflects_active_popups_and_search() {
    use ncview_rs::app::InputMode;
    let mut state = AppState::default();
    assert_eq!(state.view.input_mode(), InputMode::Normal);

    state.reduce(Command::OpenLimits);
    assert_eq!(
        state.view.input_mode(),
        InputMode::TextOverlay(Overlay::Limits)
    );

    state.reduce(Command::Quit);
    assert_eq!(state.view.input_mode(), InputMode::Normal);

    state.reduce(Command::ToggleHelp);
    assert_eq!(state.view.input_mode(), InputMode::Help);

    state.reduce(Command::Quit);
    assert_eq!(state.view.input_mode(), InputMode::Normal);

    state.reduce(Command::OpenVariableSearch);
    assert_eq!(state.view.input_mode(), InputMode::VariableSearch);

    state.reduce(Command::Quit);
    assert_eq!(state.view.input_mode(), InputMode::Normal);
}

#[test]
fn input_mode_key_events_do_not_trigger_main_shortcuts() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ncview_rs::app::InputMode;
    use ncview_rs::events::input::command_from_key_with_mode;

    let e_key = KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE);
    let c_key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);

    // In normal mode 'e' exports and 'c' cycles palette
    assert_eq!(
        command_from_key_with_mode(e_key, InputMode::Normal),
        Some(Command::ExportCurrent)
    );
    assert_eq!(
        command_from_key_with_mode(c_key, InputMode::Normal),
        Some(Command::CyclePalette)
    );

    // In text overlay mode, 'e' and 'c' are captured as input characters
    assert_eq!(
        command_from_key_with_mode(e_key, InputMode::TextOverlay(Overlay::Limits)),
        Some(Command::InputChar('e'))
    );
    assert_eq!(
        command_from_key_with_mode(c_key, InputMode::TextOverlay(Overlay::Axis)),
        Some(Command::InputChar('c'))
    );

    // In help mode, 'e' and 'c' are ignored
    assert_eq!(command_from_key_with_mode(e_key, InputMode::Help), None);
    assert_eq!(command_from_key_with_mode(c_key, InputMode::Help), None);
}

#[test]
fn backspace_in_axis_draft_clears_or_pops_input() {
    let mut state = AppState::default();
    state.view.axis_options = vec!["lat".into(), "lon".into()];
    state.reduce(Command::OpenAxisOverlay);
    assert!(state.view.axis_draft.as_ref().unwrap().replace_active);

    state.reduce(Command::DeleteInput);
    assert_eq!(state.view.axis_draft.as_ref().unwrap().x, "");
    assert!(!state.view.axis_draft.as_ref().unwrap().replace_active);

    state.reduce(Command::InputChar('y'));
    assert_eq!(state.view.axis_draft.as_ref().unwrap().x, "y");
}

#[test]
fn execute_palette_choice_runs_specified_matching_entry() {
    let mut state = AppState::default();
    state.reduce(Command::OpenCommandPalette);
    state.reduce(Command::ExecutePaletteChoice(1)); // 1 is "Cycle colormap"
    assert_eq!(
        state.view.palette,
        ncview_rs::render::colors::Palette::Plasma
    );
    assert!(state.view.overlay.is_none());
}

#[test]
fn time_navigation_does_not_change_automatic_limits() {
    let mut state = AppState::default();
    state.view.time_length = 3;
    state.view.limits = Some((1.0, 4.0));
    state.view.limits_manual = false;

    state.reduce(Command::MoveTime(1));

    assert_eq!(state.view.time_index, 1);
    assert_eq!(state.view.limits, Some((1.0, 4.0)));
}

#[test]
fn set_depth_clamps_and_syncs_cursor() {
    let mut state = AppState::default();
    state.view.depth_length = 4;
    state.view.depth_index = 0;
    state.view.depth_cursor = 2;
    state.reduce(Command::SetDepth(9));
    assert_eq!(state.view.depth_index, 3);
    assert_eq!(state.view.depth_cursor, 3);
    state.reduce(Command::SetDepth(1));
    assert_eq!(state.view.depth_index, 1);
    assert_eq!(state.view.depth_cursor, 1);
}

#[test]
fn move_depth_syncs_cursor_but_cursor_move_does_not_apply() {
    let mut state = AppState::default();
    state.view.depth_length = 5;
    state.reduce(Command::MoveDepth(1));
    assert_eq!(state.view.depth_index, 1);
    assert_eq!(state.view.depth_cursor, 1);
    state.reduce(Command::MoveDepthCursor(2));
    assert_eq!(state.view.depth_cursor, 3);
    assert_eq!(state.view.depth_index, 1);
    state.reduce(Command::MoveDepthCursor(-99));
    assert_eq!(state.view.depth_cursor, 0);
}

#[test]
fn toggle_sidebar_focus_resets_cursor_to_applied_depth() {
    let mut state = AppState::default();
    state.view.depth_length = 6;
    state.view.depth_index = 2;
    state.view.depth_cursor = 5;
    state.reduce(Command::ToggleSidebarFocus);
    assert!(state.view.sidebar_focused);
    assert_eq!(state.view.depth_cursor, 2);
    assert_eq!(state.view.input_mode(), InputMode::Sidebar);
    state.reduce(Command::ToggleSidebarFocus);
    assert!(!state.view.sidebar_focused);
    assert_eq!(state.view.input_mode(), InputMode::Normal);
}

#[test]
fn sidebar_focus_yields_to_overlays_and_search() {
    let mut state = AppState::default();
    state.view.sidebar_focused = true;
    state.view.variable_search_active = true;
    assert_eq!(state.view.input_mode(), InputMode::VariableSearch);
    state.view.variable_search_active = false;
    state.view.help_visible = true;
    assert_eq!(state.view.input_mode(), InputMode::Help);
}

#[test]
fn zero_length_depth_stays_at_index_zero() {
    let mut state = AppState::default();
    state.view.depth_length = 0;
    state.reduce(Command::SetDepth(7));
    assert_eq!(state.view.depth_index, 0);
    state.reduce(Command::MoveDepthCursor(3));
    assert_eq!(state.view.depth_cursor, 0);
}

#[test]
fn every_palette_shortcut_is_documented_in_help() {
    let help = ncview_rs::ui::help::help_text();
    for entry in ncview_rs::app::COMMAND_PALETTE {
        if entry.shortcut.is_empty() {
            continue;
        }
        assert!(
            help.contains(entry.shortcut),
            "palette shortcut {:?} for {:?} is missing from help text",
            entry.shortcut,
            entry.label,
        );
    }
}

#[test]
fn palette_exposes_depth_navigation() {
    let labels: Vec<&str> = ncview_rs::app::COMMAND_PALETTE
        .iter()
        .map(|entry| entry.label)
        .collect();
    for expected in [
        "Previous depth slice",
        "Next depth slice",
        "Focus level list",
    ] {
        assert!(
            labels.contains(&expected),
            "missing palette entry {expected}"
        );
    }
}

#[test]
fn palette_depth_entries_dispatch_depth_commands() {
    let mut state = AppState::default();
    state.view.depth_length = 4;
    let next = ncview_rs::app::COMMAND_PALETTE
        .iter()
        .position(|entry| entry.label == "Next depth slice")
        .unwrap();
    state.view.overlay = Some(Overlay::CommandPalette);
    state.reduce(Command::ExecutePaletteChoice(next));
    assert_eq!(state.view.depth_index, 1);
    assert_eq!(state.view.overlay, None);

    let focus = ncview_rs::app::COMMAND_PALETTE
        .iter()
        .position(|entry| entry.label == "Focus level list")
        .unwrap();
    state.view.overlay = Some(Overlay::CommandPalette);
    state.reduce(Command::ExecutePaletteChoice(focus));
    assert!(state.view.sidebar_focused);
    assert_eq!(state.view.overlay, None);
}

#[test]
fn help_documents_the_level_bar() {
    assert!(ncview_rs::ui::help::help_text().contains("level bar"));
}
