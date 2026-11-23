use crate::args::{parse_args, ParseOutcome};
use crate::role::Role;

fn args(slice: &[&str]) -> Vec<String> {
    slice.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn empty_args_runs_with_defaults() {
    let outcome = parse_args(args(&[])).expect("empty args ok");
    let a = match outcome {
        ParseOutcome::Run(a) => a,
        _ => panic!("expected Run"),
    };
    assert!(a.exceptions.is_none());
    assert!(a.manifest_path.is_none());
}

#[test]
fn space_separated_value() {
    let outcome = parse_args(args(&["--exceptions", "foo.toml"])).expect("ok");
    let a = match outcome {
        ParseOutcome::Run(a) => a,
        _ => panic!("expected Run"),
    };
    assert_eq!(a.exceptions.as_deref().unwrap().to_str(), Some("foo.toml"));
}

#[test]
fn equals_separated_value() {
    let outcome = parse_args(args(&["--exceptions=foo.toml"])).expect("ok");
    let a = match outcome {
        ParseOutcome::Run(a) => a,
        _ => panic!("expected Run"),
    };
    assert_eq!(a.exceptions.as_deref().unwrap().to_str(), Some("foo.toml"));
}

#[test]
fn equals_value_with_repeated_prefix_kept_intact() {
    // Regression: previous trim_start_matches stripped the prefix
    // repeatedly. strip_prefix removes it exactly once.
    let outcome = parse_args(args(&["--exceptions=--exceptions=foo"])).expect("ok");
    let a = match outcome {
        ParseOutcome::Run(a) => a,
        _ => panic!("expected Run"),
    };
    assert_eq!(
        a.exceptions.as_deref().unwrap().to_str(),
        Some("--exceptions=foo")
    );
}

#[test]
fn short_e_flag() {
    let outcome = parse_args(args(&["-e", "foo.toml"])).expect("ok");
    let a = match outcome {
        ParseOutcome::Run(a) => a,
        _ => panic!("expected Run"),
    };
    assert_eq!(a.exceptions.as_deref().unwrap().to_str(), Some("foo.toml"));
}

#[test]
fn manifest_path_space_and_equals() {
    for raw in [
        vec!["--manifest-path", "Cargo.toml"],
        vec!["--manifest-path=Cargo.toml"],
    ] {
        let owned: Vec<String> = raw.into_iter().map(String::from).collect();
        let outcome = parse_args(owned).expect("ok");
        let a = match outcome {
            ParseOutcome::Run(a) => a,
            _ => panic!("expected Run"),
        };
        assert_eq!(
            a.manifest_path.as_deref().unwrap().to_str(),
            Some("Cargo.toml")
        );
    }
}

#[test]
fn missing_value_is_an_error() {
    assert!(parse_args(args(&["--exceptions"])).is_err());
    assert!(parse_args(args(&["--manifest-path"])).is_err());
}

#[test]
fn unknown_flag_is_an_error() {
    let err = parse_args(args(&["--what"])).unwrap_err();
    assert!(err.contains("--what"), "{err}");
}

#[test]
fn help_flag_short_circuits() {
    let outcome = parse_args(args(&["--help"])).expect("ok");
    assert!(matches!(outcome, ParseOutcome::HelpRequested));
    let outcome = parse_args(args(&["-h"])).expect("ok");
    assert!(matches!(outcome, ParseOutcome::HelpRequested));
}

#[test]
fn version_flag_short_circuits() {
    let outcome = parse_args(args(&["--version"])).expect("ok");
    assert!(matches!(outcome, ParseOutcome::VersionRequested));
    let outcome = parse_args(args(&["-V"])).expect("ok");
    assert!(matches!(outcome, ParseOutcome::VersionRequested));
}

#[test]
fn format_default_is_text() {
    let outcome = parse_args(args(&[])).expect("ok");
    let a = match outcome {
        ParseOutcome::Run(a) => a,
        _ => panic!("expected Run"),
    };
    assert_eq!(a.format.as_str(), "text");
}

#[test]
fn format_json_short_and_long() {
    for raw in [
        vec!["--format", "json"],
        vec!["--format=json"],
        vec!["-f", "json"],
    ] {
        let owned: Vec<String> = raw.into_iter().map(String::from).collect();
        let outcome = parse_args(owned).expect("ok");
        let a = match outcome {
            ParseOutcome::Run(a) => a,
            _ => panic!("expected Run"),
        };
        assert_eq!(a.format.as_str(), "json");
    }
}

#[test]
fn format_text_explicit_is_accepted() {
    let outcome = parse_args(args(&["--format=text"])).expect("ok");
    let a = match outcome {
        ParseOutcome::Run(a) => a,
        _ => panic!("expected Run"),
    };
    assert_eq!(a.format.as_str(), "text");
}

#[test]
fn format_unknown_is_an_error() {
    let err = parse_args(args(&["--format=yaml"])).unwrap_err();
    assert!(err.contains("yaml"), "{err}");
    let err = parse_args(args(&["-f", "xml"])).unwrap_err();
    assert!(err.contains("xml"), "{err}");
}

#[test]
fn format_missing_value_is_an_error() {
    assert!(parse_args(args(&["--format"])).is_err());
    assert!(parse_args(args(&["-f"])).is_err());
}

#[test]
fn explain_parses_role() {
    let outcome = parse_args(args(&["explain", "usecase"])).expect("ok");
    match outcome {
        ParseOutcome::Explain(role) => assert_eq!(role, Role::Usecase),
        _ => panic!("expected Explain"),
    }
}

#[test]
fn explain_unknown_role_is_an_error() {
    let err = parse_args(args(&["explain", "nonsense"])).unwrap_err();
    assert!(err.contains("nonsense"), "{err}");
    for &r in Role::ALL {
        assert!(
            err.contains(r.as_str()),
            "explain's error must offer `{}`, got: {err}",
            r.as_str()
        );
    }
}

#[test]
fn explain_missing_role_is_an_error() {
    assert!(parse_args(args(&["explain"])).is_err());
}

#[test]
fn help_documents_every_enforced_axis() {
    // Guards the exact drift that prompted this: `main` enforces both the
    // role matrix and context isolation, so `--help` must document both.
    let help: String = crate::help::help_text();

    // Role axis.
    assert!(help.contains("ROLES"), "help must list the roles");
    // Rendered from `Role::ALL`, so a new role can never be silently
    // absent from the help the way `kernel` would have been.
    for role in Role::ALL {
        assert!(
            help.contains(role.as_str()),
            "help must name the `{}` role",
            role.as_str()
        );
    }
    assert!(
        help.contains("role = \"domain\""),
        "help must show the role tag"
    );

    // Context axis — the part that was missing.
    assert!(
        help.contains("CONTEXT ISOLATION"),
        "help must document the context axis"
    );
    assert!(
        help.contains("context = "),
        "help must show the context tag"
    );
    assert!(
        help.contains("\"shared\""),
        "help must name the reserved shared context"
    );
    assert!(
        help.contains("partial adoption"),
        "help must state the all-or-nothing adoption rule"
    );
    assert!(
        help.contains("is exempt"),
        "help must state the context axis's one exemption"
    );

    // Exceptions cover both axes.
    assert!(
        help.contains("axis = \"context\""),
        "help must show the context exception axis key"
    );
}
