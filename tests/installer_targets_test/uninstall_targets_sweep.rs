use super::common::*;

#[test]
fn sweeps_every_agent_it_was_installed_on_and_reports_removed_for_each_global() {
    with_fixture("uninstall-sweep-all", |_| {
        for target in all_targets() {
            if target.supports_location(Location::Global) {
                target.install(Location::Global, install_options(true));
            }
        }
        let reports = uninstall_targets(all_targets(), Location::Global);
        for target in all_targets() {
            let report = reports
                .iter()
                .find(|report| report.id == target.id())
                .unwrap();
            assert_eq!(
                report.status,
                UninstallStatus::Removed,
                "{}",
                target.id().as_str()
            );
            assert!(!report.removed_paths.is_empty(), "{}", target.id().as_str());
            assert!(
                !target.detect(Location::Global).already_configured,
                "{}",
                target.id().as_str()
            );
        }
    });
}

#[test]
fn safe_on_clean_slate_every_agent_reports_not_configured_nothing_removed() {
    with_fixture("uninstall-sweep-clean", |_| {
        let reports = uninstall_targets(all_targets(), Location::Global);
        for report in reports {
            assert_eq!(report.status, UninstallStatus::NotConfigured);
            assert!(report.removed_paths.is_empty());
        }
    });
}

#[test]
fn reports_removed_only_for_agents_that_were_actually_configured() {
    with_fixture("uninstall-sweep-one", |_| {
        target("claude").install(Location::Global, install_options(true));
        let reports = uninstall_targets(all_targets(), Location::Global);
        let claude = reports
            .iter()
            .find(|report| report.id == TargetId::Claude)
            .unwrap();
        assert_eq!(claude.status, UninstallStatus::Removed);
        assert_eq!(claude.display_name, target("claude").display_name());
        for report in reports
            .iter()
            .filter(|report| report.id != TargetId::Claude)
        {
            assert_eq!(
                report.status,
                UninstallStatus::NotConfigured,
                "{:?}",
                report.id
            );
        }
    });
}

#[test]
fn marks_global_only_agents_as_unsupported_for_local_sweep_and_never_touches_them() {
    with_fixture("uninstall-sweep-local", |_| {
        let reports = uninstall_targets(all_targets(), Location::Local);
        for target in all_targets() {
            let report = reports
                .iter()
                .find(|report| report.id == target.id())
                .unwrap();
            if target.supports_location(Location::Local) {
                assert_eq!(report.status, UninstallStatus::NotConfigured);
            } else {
                assert_eq!(report.status, UninstallStatus::Unsupported);
                assert!(report.removed_paths.is_empty());
                assert!(report.notes[0].contains("global-only"));
            }
        }
    });
}

#[test]
fn idempotent_second_sweep_finds_nothing_left_to_remove() {
    with_fixture("uninstall-sweep-idempotent", |_| {
        for target in all_targets() {
            if target.supports_location(Location::Global) {
                target.install(Location::Global, install_options(true));
            }
        }
        let first = uninstall_targets(all_targets(), Location::Global);
        assert!(
            first
                .iter()
                .any(|report| report.status == UninstallStatus::Removed)
        );
        let second = uninstall_targets(all_targets(), Location::Global);
        for report in second {
            assert_eq!(report.status, UninstallStatus::NotConfigured);
            assert!(report.removed_paths.is_empty());
        }
    });
}

#[test]
fn target_subset_removes_only_chosen_agents_leaving_siblings_configured() {
    with_fixture("uninstall-sweep-subset", |_| {
        target("claude").install(Location::Global, install_options(true));
        target("cursor").install(Location::Global, install_options(true));
        let reports = uninstall_targets(
            resolve_target_flag("claude", Location::Global).unwrap(),
            Location::Global,
        );
        assert_eq!(
            reports.iter().map(|report| report.id).collect::<Vec<_>>(),
            vec![TargetId::Claude]
        );
        assert_eq!(reports[0].status, UninstallStatus::Removed);
        assert!(target("cursor").detect(Location::Global).already_configured);
        assert!(!target("claude").detect(Location::Global).already_configured);
    });
}
