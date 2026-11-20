use ramview_core::algorithms::SquarifiedTreemap;
use ramview_core::memory_calculator::MemoryCalculator;
use ramview_core::models::{format_bytes, ProcessMemoryInfo};
use ramview_core::process_filter::{apply_filter, group_processes, is_system_process};
use ramview_core::terminate::can_terminate;

/// Port of tests/RAMView.Tests/MemoryCalculatorTests.cs
mod memory_calculator_tests {
    use super::*;

    fn process(pid: u32, name: &str, bytes: i64) -> ProcessMemoryInfo {
        ProcessMemoryInfo::new(pid, name, bytes)
    }

    #[test]
    fn calculate_layout_weights_applies_min_memory_floor() {
        let min_floor_bytes = 20 * 1024 * 1024;

        let processes = vec![
            process(1, "TinyApp", 2 * 1024 * 1024),
            process(2, "BigApp", 200 * 1024 * 1024),
        ];

        let weights = MemoryCalculator::calculate_layout_weights(&processes, min_floor_bytes, 0.90);
        let tiny = weights
            .iter()
            .find(|(p, _)| p.process_name == "TinyApp")
            .expect("TinyApp must be weighted");

        assert!(
            tiny.1 >= min_floor_bytes as f64,
            "Tiny app weight must meet or exceed minimum floor"
        );
    }

    #[test]
    fn calculate_layout_weights_applies_anti_dominance_rule() {
        let max_share_ratio = 0.60;

        let processes = vec![
            process(1, "DominantChrome", 9000 * 1024 * 1024),
            process(2, "SmallApp", 500 * 1024 * 1024),
            process(3, "OtherApp", 500 * 1024 * 1024),
        ];

        let weights =
            MemoryCalculator::calculate_layout_weights(&processes, 20 * 1024 * 1024, max_share_ratio);

        let total: f64 = weights.iter().map(|(_, weight)| *weight).sum();
        let dominant = weights
            .iter()
            .find(|(p, _)| p.process_name == "DominantChrome")
            .expect("dominant process must be present");

        let dominant_share = dominant.1 / total;
        assert!(
            dominant_share <= max_share_ratio + 0.01,
            "Dominant process share {dominant_share:.3} must not exceed {max_share_ratio:.3}"
        );
    }

    #[test]
    fn calculate_layout_weights_maintains_stable_sorting() {
        let processes = vec![
            process(2, "Beta", 100 * 1024 * 1024),
            process(1, "Alpha", 100 * 1024 * 1024),
            process(3, "Gamma", 500 * 1024 * 1024),
        ];

        let weights = MemoryCalculator::calculate_layout_weights(&processes, 20 * 1024 * 1024, 0.60);

        assert_eq!(weights[0].0.process_name, "Gamma");
        assert_eq!(weights[1].0.process_name, "Alpha");
        assert_eq!(weights[2].0.process_name, "Beta");
    }
}

/// Port of tests/RAMView.Tests/SquarifiedTreemapTests.cs
mod squarified_treemap_tests {
    use super::*;

    fn fake_process(name: &str, working_set_mb: i64) -> ProcessMemoryInfo {
        ProcessMemoryInfo::new(1234, name, working_set_mb * 1024 * 1024)
    }

    #[test]
    fn calculate_layout_empty_items_returns_empty() {
        let layout = SquarifiedTreemap::calculate_layout(&[], 500.0, 300.0, 2.0);
        assert!(layout.is_empty());
    }

    #[test]
    fn calculate_layout_zero_dimensions_returns_empty() {
        let items = vec![(fake_process("test", 100), 100.0)];
        let layout = SquarifiedTreemap::calculate_layout(&items, 0.0, 300.0, 2.0);
        assert!(layout.is_empty());
    }

    #[test]
    fn calculate_layout_single_item_fills_canvas_roughly() {
        let items = vec![(fake_process("solo", 500), 500.0)];
        let layout = SquarifiedTreemap::calculate_layout(&items, 400.0, 200.0, 0.0);

        assert_eq!(layout.len(), 1);
        let rect = &layout[0];
        assert!((rect.x - 0.0).abs() < 1.0);
        assert!((rect.y - 0.0).abs() < 1.0);
        assert!((rect.width - 400.0).abs() < 1.0);
        assert!((rect.height - 200.0).abs() < 1.0);
    }

    #[test]
    fn calculate_layout_multiple_items_no_overlaps_and_bounded() {
        let canvas_w = 600.0;
        let canvas_h = 400.0;

        let items = vec![
            (fake_process("Chrome", 2000), 2000.0),
            (fake_process("IDE", 1500), 1500.0),
            (fake_process("Discord", 800), 800.0),
            (fake_process("Spotify", 400), 400.0),
            (fake_process("Terminal", 200), 200.0),
            (fake_process("Notepad", 50), 50.0),
        ];

        let layout = SquarifiedTreemap::calculate_layout(&items, canvas_w, canvas_h, 2.0);

        assert_eq!(items.len(), layout.len());

        for rect in &layout {
            assert!(rect.x >= 0.0, "X {} >= 0", rect.x);
            assert!(rect.y >= 0.0, "Y {} >= 0", rect.y);
            assert!(
                rect.x + rect.width <= canvas_w + 1.0,
                "X+W {} <= {}",
                rect.x + rect.width,
                canvas_w
            );
            assert!(
                rect.y + rect.height <= canvas_h + 1.0,
                "Y+H {} <= {}",
                rect.y + rect.height,
                canvas_h
            );
            assert!(rect.width > 0.0, "Width > 0");
            assert!(rect.height > 0.0, "Height > 0");
        }
    }

    #[test]
    fn calculate_layout_preserves_ordering_of_highest_weight() {
        let items = vec![
            (fake_process("Huge", 10000), 10000.0),
            (fake_process("Medium", 3000), 3000.0),
            (fake_process("Small", 500), 500.0),
        ];

        let layout = SquarifiedTreemap::calculate_layout(&items, 800.0, 600.0, 0.0);

        let huge = layout
            .iter()
            .find(|rect| rect.process.process_name == "Huge")
            .expect("Huge must be laid out");
        let small = layout
            .iter()
            .find(|rect| rect.process.process_name == "Small")
            .expect("Small must be laid out");

        assert!(
            huge.width * huge.height > small.width * small.height,
            "Highest weight item must produce larger area"
        );
    }
}

/// Port of tests/RAMView.Tests/ProcessTerminatorTests.cs
mod process_terminator_tests {
    use super::*;

    #[test]
    fn can_terminate_critical_processes_refuses_with_reason() {
        let cases = [
            (0u32, "Idle"),
            (4, "System"),
            (1234, "csrss.exe"),
            (1234, "csrss"),
            (5678, "wininit.exe"),
            (9101, "services.exe"),
            (1112, "smss.exe"),
            (1314, "lsass.exe"),
            (1516, "dwm.exe"),
        ];

        for (pid, name) in cases {
            let result = can_terminate(pid, name);
            assert!(result.is_err(), "Process '{name}' with PID {pid} must be protected.");
            let reason = result.unwrap_err();
            assert!(!reason.is_empty());
            assert!(reason.contains("protected Windows core process"));
        }
    }

    #[test]
    fn can_terminate_user_processes_allows_termination() {
        let cases = [
            (5000u32, "notepad.exe"),
            (6000, "chrome.exe"),
            (7000, "CustomApp"),
        ];

        for (pid, name) in cases {
            let result = can_terminate(pid, name);
            assert!(result.is_ok(), "User process '{name}' should be allowed to terminate.");
            assert_eq!(result.unwrap(), ());
        }
    }
}

/// Formatting and filter/grouping parity checks (ProcessMemoryInfo / ProcessMonitor).
mod formatting_and_filter_tests {
    use super::*;

    #[test]
    fn format_bytes_matches_dotnet_shapes() {
        assert_eq!(format_bytes(0), "0 MB");
        assert_eq!(format_bytes(-5), "0 MB");
        assert_eq!(format_bytes(2 * 1024 * 1024), "2.0 MB");
        assert_eq!(format_bytes(512 * 1024), "512 KB");
        assert_eq!(format_bytes(3 * 1024 * 1024 * 1024), "3.00 GB");
    }

    #[test]
    fn formatted_percentage_uses_one_decimal() {
        let mut process = ProcessMemoryInfo::new(1, "App", 1024);
        process.percentage_of_tracked = 15.44;
        assert_eq!(process.formatted_percentage(), "15.4%");
    }

    #[test]
    fn is_system_process_matches_process_monitor_list() {
        assert!(is_system_process(0, "Idle"));
        assert!(is_system_process(4, "System"));
        assert!(is_system_process(1000, "svchost"));
        assert!(is_system_process(1000, "Memory Compression"));
        assert!(!is_system_process(1000, "notepad"));
    }

    #[test]
    fn grouping_aggregates_by_name_ignoring_case() {
        let mut first = ProcessMemoryInfo::new(1, "chrome", 100 * 1024 * 1024);
        first.is_responding = true;
        let mut second = ProcessMemoryInfo::new(2, "Chrome", 300 * 1024 * 1024);
        second.is_responding = false;
        let mut third = ProcessMemoryInfo::new(3, "notepad", 50 * 1024 * 1024);
        third.is_responding = true;

        let grouped = group_processes(vec![first, second, third]);
        assert_eq!(grouped.len(), 2);

        let chrome = grouped
            .iter()
            .find(|p| p.process_name.eq_ignore_ascii_case("chrome"))
            .expect("chrome group");
        assert_eq!(chrome.instance_count, 2);
        assert_eq!(chrome.working_set64, 400 * 1024 * 1024);
        assert!(!chrome.is_responding);
    }

    #[test]
    fn highest_filter_keeps_top_25() {
        let processes: Vec<ProcessMemoryInfo> = (0..40)
            .map(|index| ProcessMemoryInfo::new(index, format!("p{index}"), (index as i64 + 1) * 1024))
            .collect();

        let filtered = apply_filter(processes, "Highest");
        assert_eq!(filtered.len(), 25);
        assert_eq!(filtered[0].working_set64, 40 * 1024);
    }

    #[test]
    fn application_filter_excludes_system_processes() {
        let processes = vec![
            ProcessMemoryInfo::new(4, "System", 1024),
            ProcessMemoryInfo::new(100, "notepad", 1024),
            ProcessMemoryInfo::new(101, "svchost", 1024),
        ];

        let filtered = apply_filter(processes, "Applications");
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].process_name, "notepad");
    }
}

// Port of tests/RAMView.Tests/SettingsServiceTests.cs lives in ramview-platform,
// since it needs the filesystem and the real settings service.
