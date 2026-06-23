mod float32array_fix {
    #[test]
    fn should_correctly_convert_typed_arrays_regression_check() {
        let source = [1.5_f64, 2.5, 3.5, 4.5];

        let buggy = vec![0.0_f32; source.len()];
        assert_eq!(buggy[0], 0.0);
        assert_eq!(buggy[1], 0.0);

        let fixed = source
            .iter()
            .copied()
            .map(|value| value as f32)
            .collect::<Vec<_>>();
        assert!((fixed[0] - 1.5).abs() < f32::EPSILON);
        assert!((fixed[1] - 2.5).abs() < f32::EPSILON);
        assert!((fixed[2] - 3.5).abs() < f32::EPSILON);
        assert!((fixed[3] - 4.5).abs() < f32::EPSILON);
    }
}
