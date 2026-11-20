use rustcam_media::domain::hierarchy::expand_group_ids;

fn groups() -> Vec<(i64, Option<i64>)> {
    // 1 → 2 → 3，4 是另一棵子树
    vec![(1, None), (2, Some(1)), (3, Some(2)), (4, None)]
}

#[test]
fn group_expansion_includes_descendants() {
    let ids = expand_group_ids(&groups(), 1);
    assert!(ids.contains(&1));
    assert!(ids.contains(&2));
    assert!(ids.contains(&3));
    assert!(!ids.contains(&4));
    assert_eq!(ids.len(), 3);
}

#[test]
fn group_expansion_of_unknown_group_returns_itself() {
    let ids = expand_group_ids(&groups(), 999);
    assert_eq!(ids, vec![999]);
}

#[test]
fn group_expansion_survives_cycles() {
    let cyclic = vec![(1, Some(2)), (2, Some(1)), (3, Some(1))];
    let ids = expand_group_ids(&cyclic, 1);
    assert!(ids.contains(&1));
    assert!(ids.contains(&2));
    assert!(ids.contains(&3));
    assert_eq!(ids.len(), 3);
}
