#[test]
fn test_join_classes() {
    use stylance::JoinClasses;
    assert_eq!(
        (
            "one",
            Some("two"),
            false.then_some("three"),
            true.then_some("four"),
            &String::from("five"),
            Some(&String::from("six")),
            &("seven", "eight").join_classes()
        )
            .join_classes(),
        "one two four five six seven eight"
    );
}

#[test]
fn test_classes_macro() {
    use stylance::classes;
    assert_eq!(
        classes!(
            "one",
            Some("two"),
            false.then_some("three"),
            true.then_some("four"),
            &String::from("five"),
            Some(&String::from("six")),
            &classes!("seven", "eight")
        ),
        "one two four five six seven eight"
    );
}
