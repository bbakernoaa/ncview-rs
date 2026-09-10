use ncview_rs::{
    app::{AppState, Command, Effect, GridMode, LimitField, Overlay},
    data::Variable,
    ui::sidebar::filter_variables,
};

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
    let effect = state.reduce(Command::SelectVariable(0));
    assert!(matches!(effect, Some(Effect::ReadSlice { .. })));
    assert_eq!(state.view.generation.0, 1);
    assert_eq!(state.view.selected_variable.as_deref(), Some("temperature"));
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
    assert_eq!(state.view.overlay, Some(Overlay::TimeSeries));
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
