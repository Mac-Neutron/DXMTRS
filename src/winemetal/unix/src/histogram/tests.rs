use super::*;

/// A list of `kind` that walked `records`.
fn walked(kind: Kind, records: &[u16]) -> List {
    let mut list = List::new(kind);
    for &record in records {
        list.record(record);
    }
    list
}

fn text(histogram: &Histogram<Vec<u8>>) -> &str {
    core::str::from_utf8(&histogram.out).expect("the lines are text")
}

#[test]
fn lines_count_since_the_last_commit() {
    let mut histogram = Histogram::new(Vec::new());
    histogram.add(walked(Kind::Render, &[0, 5, 5, 9]));
    histogram.add(walked(Kind::Blit, &[2]));
    histogram.add(walked(Kind::Render, &[5, 7]));
    histogram.commit();
    assert_eq!(
        text(&histogram),
        "render 0 1\nrender 5 3\nrender 7 1\nrender 9 1\nblit 2 1\n"
    );
    histogram.add(walked(Kind::Compute, &[1, 1]));
    histogram.commit();
    assert_eq!(
        text(&histogram),
        "render 0 1\nrender 5 3\nrender 7 1\nrender 9 1\nblit 2 1\ncompute 1 2\n"
    );
}

#[test]
fn a_commit_with_nothing_counted_writes_nothing() {
    let mut histogram = Histogram::new(Vec::new());
    histogram.commit();
    histogram.add(walked(Kind::Compute, &[]));
    histogram.commit();
    assert_eq!(text(&histogram), "");
}

#[test]
fn kinds_are_written_render_compute_blit() {
    let mut histogram = Histogram::new(Vec::new());
    histogram.add(walked(Kind::Blit, &[1]));
    histogram.add(walked(Kind::Compute, &[1]));
    histogram.add(walked(Kind::Render, &[1]));
    histogram.commit();
    assert_eq!(text(&histogram), "render 1 1\ncompute 1 1\nblit 1 1\n");
}
