use super::Fixture;

fn place_pointer(f: &mut Fixture, output: &smithay::output::Output, local: (f64, f64)) {
    let geometry = f
        .clarion()
        .global_space
        .output_geometry(output)
        .unwrap()
        .loc
        .to_f64();
    f.clarion()
        .seat
        .get_pointer()
        .unwrap()
        .set_location(geometry + smithay::utils::Point::from(local));
    f.clarion().refresh_pointer_outputs();
}

#[test]
fn pointer_outputs_follow_the_cursor_between_outputs() {
    let mut f = Fixture::new();
    f.add_output(1, (640, 400));
    f.add_output(2, (640, 400));
    let first = f.clarion_output(1);
    let second = f.clarion_output(2);

    place_pointer(&mut f, &first, (10., 10.));
    assert_eq!(
        f.clarion()
            .outputs_overlapping_pointer
            .iter()
            .map(|output| output.name())
            .collect::<Vec<_>>(),
        [first.name()]
    );

    place_pointer(&mut f, &second, (10., 10.));
    assert_eq!(
        f.clarion()
            .outputs_overlapping_pointer
            .iter()
            .map(|output| output.name())
            .collect::<Vec<_>>(),
        [second.name()]
    );
}
