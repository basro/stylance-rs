#[test]
fn test_join_classes() {
    use stylance::{JoinClasses, NormalizedClass};

    assert_eq!(
        ([
            Into::<NormalizedClass>::into("one"),
            Some("two").into(),
            false.then_some("three").into(),
            true.then_some("four").into(),
            (&String::from("five")).into(),
            Some(&String::from("six")).into(),
            (&(&["seven".into(), "eight".into()]).join_classes()).into(),
        ])
        .join_classes(),
        "one two four five six seven eight"
    );
}

#[test]
fn test_classes_macro_none() {
    use stylance::classes;
    assert_eq!(classes!(), "");
}

#[test]
fn test_classes_macro_one() {
    use stylance::classes;
    assert_eq!(classes!("one"), "one");
    assert_eq!(classes!(Some("one")), "one");
    assert_eq!(classes!(false.then_some("one")), "");
}

#[test]
fn test_classes_macro_many() {
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
