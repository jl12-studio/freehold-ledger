use crate::model::Kind;
use serde::Serialize;

#[derive(Debug, Serialize, Clone, Copy)]
pub struct Category {
    pub key: &'static str,
    pub schedule_c_line: Option<&'static str>,
    pub label: &'static str,
    pub kind: Kind,
}

const fn exp(key: &'static str, line: Option<&'static str>, label: &'static str) -> Category {
    Category { key, schedule_c_line: line, label, kind: Kind::Expense }
}

static CATEGORIES: [Category; 25] = [
    exp("advertising", Some("8"), "Advertising"),
    exp("car_truck", Some("9"), "Car and truck expenses"),
    exp("commissions_fees", Some("10"), "Commissions and fees"),
    exp("contract_labor", Some("11"), "Contract labor"),
    exp("depreciation_section_179", Some("13"), "Depreciation and section 179"),
    exp("employee_benefits", Some("14"), "Employee benefit programs"),
    exp("insurance", Some("15"), "Insurance (other than health)"),
    exp("interest_mortgage", Some("16a"), "Interest: mortgage"),
    exp("interest_other", Some("16b"), "Interest: other"),
    exp("legal_professional", Some("17"), "Legal and professional services"),
    exp("office_expense", Some("18"), "Office expense"),
    exp("pension_profit_sharing", Some("19"), "Pension and profit-sharing plans"),
    exp("rent_vehicles_equipment", Some("20a"), "Rent or lease: vehicles, machinery, equipment"),
    exp("rent_other", Some("20b"), "Rent or lease: other business property"),
    exp("repairs_maintenance", Some("21"), "Repairs and maintenance"),
    exp("supplies", Some("22"), "Supplies"),
    exp("taxes_licenses", Some("23"), "Taxes and licenses"),
    exp("travel", Some("24a"), "Travel"),
    exp("meals", Some("24b"), "Deductible meals"),
    exp("utilities", Some("25"), "Utilities"),
    exp("wages", Some("26"), "Wages"),
    exp("other_expense", Some("27a"), "Other expenses"),
    exp("uncategorized", None, "Needs a category"),
    Category { key: "gross_receipts", schedule_c_line: Some("1"), label: "Gross receipts or sales", kind: Kind::Income },
    Category { key: "other_income", schedule_c_line: Some("6"), label: "Other income", kind: Kind::Income },
];

pub fn categories() -> &'static [Category] {
    &CATEGORIES
}

pub fn category(key: &str) -> Option<&'static Category> {
    CATEGORIES.iter().find(|c| c.key == key)
}

pub(crate) fn valid_keys(kind: Kind) -> Vec<&'static str> {
    CATEGORIES.iter().filter(|c| c.kind == kind).map(|c| c.key).collect()
}
