mod common;
use ledger_core::*;

#[test]
fn s_seed_1_demo_payment_methods() {
    let (_d, mut l) = common::seeded();
    seed_demo_payment_methods(&mut l).unwrap();
    assert_eq!(l.info().unwrap().transaction_count, 10);
    let all = l.list(&Filter { include_voided: true, ..Default::default() }).unwrap();
    assert_eq!(all.iter().filter(|t| t.void.is_some()).count(), 2);
    assert_eq!(all.iter().filter(|t| !t.classification_history.is_empty()).count(), 1);
    let nick = |id: i64| l.get(id).unwrap().payment_method.map(|p| p.nickname);
    for (id, want) in [
        (1, Some("Business Visa")), (2, Some("Checking")), (3, Some("Checking")), (4, Some("Cash")),
        (5, None), (6, None), (7, None), (8, None), (9, Some("Old Amex")), (10, Some("Business Visa")),
    ] {
        assert_eq!(nick(id).as_deref(), want, "entry {id}");
    }
    let methods = l.payment_methods(true).unwrap();
    let names: Vec<&str> = methods.iter().map(|m| m.nickname.as_str()).collect();
    assert_eq!(names, ["Business Visa", "Cash", "Checking", "Old Amex"]);
    let ids: Vec<i64> = methods.iter().map(|m| m.id).collect();
    assert_eq!(ids, [1, 3, 2, 4]);
    assert!(methods[0].is_default && !methods[3].is_default);
    assert!(methods[3].retired);
    let visa = l.get(1).unwrap().payment_method.unwrap();
    assert_eq!((visa.kind, visa.last4.as_deref()), (PaymentKind::Card, Some("4242")));
    assert!(l.get(9).unwrap().payment_method.unwrap().retired);
}

#[test]
fn s_seed_2_demo_test_shop() {
    let (_d, mut l) = common::seeded();
    seed_demo_payment_methods(&mut l).unwrap();
    seed_demo_businesses(&mut l).unwrap();
    assert_eq!(l.info().unwrap().transaction_count, 12);
    assert_eq!(l.info().unwrap().current_business_id, Some(1));
    let all = l.businesses(true).unwrap();
    let names: Vec<(&str, bool, i64)> = all.iter().map(|b| (b.name.as_str(), b.is_test, b.transaction_count)).collect();
    assert_eq!(names, [("My business", false, 10), ("Test shop", true, 2)]);
    let shop = l.list(&Filter { business_id: Some(2), include_voided: true, ..Default::default() }).unwrap();
    let ids: Vec<i64> = shop.iter().map(|t| t.id).collect();
    assert_eq!(ids, [12, 11]);
    assert_eq!(l.get(11).unwrap().payment_method.unwrap().nickname, "Test card");
    let key = BusinessKey::Id(2);
    let methods = l.payment_methods_in(Some(&key), true).unwrap();
    assert_eq!((methods.len(), methods[0].id, methods[0].is_default), (1, 5, true));
    assert_eq!(l.payment_methods(true).unwrap().len(), 4);
    assert_eq!(l.income_sources_in(2).unwrap(), ["Test buyer"]);
}
