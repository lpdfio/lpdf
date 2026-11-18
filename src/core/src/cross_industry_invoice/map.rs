//! An invoice map: an XML document that names where each value a template binds sits in a Cross Industry Invoice.
//!
//! ```xml
//! <cross-industry-invoice-map prefix="invoice">
//!   <value name="number" path="rsm:ExchangedDocument/ram:ID" required="true"/>
//!   <group name="seller" path="rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty">
//!     <value name="name" path="ram:Name" required="true"/>
//!   </group>
//!   <list name="lines" path="rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem" required="true">
//!     <value name="net" path="ram:SpecifiedLineTradeSettlement/…/ram:LineTotalAmount" type="amount"/>
//!   </list>
//! </cross-industry-invoice-map>
//! ```
//!
//! The map is checked whole when a call starts, before any invoice is read: its form, and each path against the elements
//! FeRD's EXTENDED schema allows at that place (`schema.rs`). A `<value>` whose path the schema lets repeat, and that a
//! test in `[ ]` doesn't narrow, is an error: the engine never guesses which of several to take.

use std::collections::HashSet;

use super::path::{self, Operand, Path, Subject};
use super::schema::{SchemaChild, ROOT};

/// The name of the map's root element.
const ROOT_ELEMENT: &str = "cross-industry-invoice-map";

/// An invoice map, read and checked.
#[derive(Debug)]
pub(crate) struct InvoiceMap {
    /// What a template writes before an invoice value: `invoice` in `invoice:seller.name`.
    pub prefix:  String,
    pub entries: Vec<Entry>,
}

/// An entry of a map: a value, a group or a list, named and placed by its path.
#[derive(Debug)]
pub(crate) struct Entry {
    pub name:      String,
    /// The name with the names of the groups and the list around it, without the prefix: `seller.contacts.email`.
    pub full_name: String,
    pub path:      Path,
    pub required:  bool,
    pub kind:      Kind,
}

/// What an entry is.
#[derive(Debug)]
pub(crate) enum Kind {
    /// A value, with its type when it has one.
    Value(Option<ValueType>),
    /// An object of the entries in it, from the one element its path finds.
    Group(Vec<Entry>),
    /// An array with an object of the entries in it for each element its path finds.
    List(Vec<Entry>),
}

/// The type of a value: its normal form, and the `format` function of the call that writes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValueType {
    /// A date: the invoice's `20261031` becomes `2026-10-31`.
    Date,
    /// An amount, as the invoice writes it.
    Amount,
    /// A code, as the invoice writes it: `H87`, `S`, `381`.
    Code,
}

impl ValueType {
    fn parse(text: &str) -> Option<Self> {
        match text {
            "date" => Some(ValueType::Date),
            "amount" => Some(ValueType::Amount),
            "code" => Some(ValueType::Code),
            _ => None,
        }
    }

    /// The type as a map writes it, and the name of its `format` function.
    pub(crate) fn name(self) -> &'static str {
        match self {
            ValueType::Date => "date",
            ValueType::Amount => "amount",
            ValueType::Code => "code",
        }
    }
}

/// What holds an entry: the map's root, a group or a list. A list holds values only, for now.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Holder {
    Root,
    Group,
    List,
}

/// Reads a map and checks it whole, or says what is wrong in it, with its line.
pub(crate) fn parse(text: &str) -> Result<InvoiceMap, String> {
    let document = super::parse_xml(text, "the invoice map")?;
    let root = document.root_element();
    if root.tag_name().name() != ROOT_ELEMENT || root.tag_name().namespace().is_some() {
        return Err(format!(
            "the invoice map's root is <{}>: an invoice map is a <{ROOT_ELEMENT} prefix=\"invoice\"> of <value>, <group> and \
            <list> entries",
            root.tag_name().name()
        ));
    }
    if let Some(attribute) = root.attributes().find(|a| a.name() != "prefix" || a.namespace().is_some()) {
        return Err(format!("the invoice map's <{ROOT_ELEMENT}> has no attribute {}: it has prefix", attribute.name()));
    }
    let prefix = root.attribute("prefix").ok_or_else(|| {
        format!(
            "the invoice map's <{ROOT_ELEMENT}> has no prefix, which a template writes before an invoice value, as in \
            invoice:seller.name: add prefix=\"invoice\""
        )
    })?;
    if !crate::data::split_prefix(&format!("{prefix}:")).is_some_and(|(p, _)| p == prefix) {
        return Err(format!(
            "the invoice map's prefix '{prefix}' is not a name of letters, digits, - and _ that starts with a letter, \
            such as invoice"
        ));
    }
    let entries = read_entries(&document, root, &ROOT, "", Holder::Root)?;
    if entries.is_empty() {
        return Err("the invoice map has no entries: add a <value>, a <group> or a <list>".to_string());
    }
    Ok(InvoiceMap { prefix: prefix.to_string(), entries })
}

/// The entries in `node`, an element of the map whose path ends at `element` of the invoice.
fn read_entries(
    document: &roxmltree::Document, node: roxmltree::Node, element: &'static SchemaChild, outer: &str, holder: Holder,
) -> Result<Vec<Entry>, String> {
    let mut entries = Vec::new();
    let mut names = HashSet::new();
    for child in node.children() {
        let line = document.text_pos_at(child.range().start).row;
        if child.is_text() {
            let text = child.text().unwrap_or_default().trim();
            if !text.is_empty() {
                return Err(format!("the invoice map, line {line}: text '{text}' outside an entry: a map holds <value>, <group> and <list> only"));
            }
            continue;
        }
        if !child.is_element() {
            continue;
        }
        let entry = read_entry(document, child, line, element, outer, holder)?;
        if !names.insert(entry.name.clone()) {
            return Err(format!("the invoice map, line {line}: two entries are named '{}': give each a name of its own", entry.full_name));
        }
        entries.push(entry);
    }
    Ok(entries)
}

/// One entry on `line` of the map, checked: its form, then its path from `element`, the place of the entry around it.
/// Its errors name the line, and the entry once it has a name.
fn read_entry(
    document: &roxmltree::Document, node: roxmltree::Node, line: u32, element: &'static SchemaChild, outer: &str,
    holder: Holder,
) -> Result<Entry, String> {
    let on_line = |reason: String| format!("the invoice map, line {line}: {reason}");
    let tag = node.tag_name().name();
    let attributes: &[&str] = match tag {
        "value" => &["name", "path", "type", "required"],
        "group" | "list" => &["name", "path", "required"],
        _ => return Err(on_line(format!("<{tag}> is not an entry of an invoice map: write <value>, <group> or <list>"))),
    };
    let name = node.attribute("name").ok_or_else(|| on_line(format!("a <{tag}> has no name")))?;
    if !is_entry_name(name) {
        return Err(on_line(format!(
            "<{tag} name=\"{name}\">: a name is letters, digits, - and _, and starts with a letter or _, such as vatId"
        )));
    }
    let full_name = if outer.is_empty() { name.to_string() } else { format!("{outer}.{name}") };
    let in_entry = |reason: String| on_line(format!("entry '{full_name}': {reason}"));
    if holder == Holder::List && tag != "value" {
        return Err(in_entry(format!(
            "a <{tag}> can't be inside the list '{outer}': a list holds values only. Put the {tag} beside the list"
        )));
    }
    if let Some(attribute) = node.attributes().find(|a| !attributes.contains(&a.name()) || a.namespace().is_some()) {
        return Err(in_entry(format!("<{tag}> has no attribute {}: it has {}", attribute.name(), attributes.join(", "))));
    }
    let path_text = node.attribute("path").ok_or_else(|| in_entry(format!("<{tag}> has no path")))?;
    let path = path::parse(path_text).map_err(in_entry)?;
    let required = match node.attribute("required") {
        None | Some("false") => false,
        Some("true") => true,
        Some(other) => return Err(in_entry(format!("required=\"{other}\" is not true or false"))),
    };
    let kind = match tag {
        "value" => {
            if let Some(inner) = node.children().find(roxmltree::Node::is_element) {
                return Err(in_entry(format!(
                    "a <value> holds no entries, and this one holds a <{}>: make it a <group>",
                    inner.tag_name().name()
                )));
            }
            let value_type = match node.attribute("type") {
                None => None,
                Some(text) => Some(ValueType::parse(text).ok_or_else(|| {
                    in_entry(format!("type=\"{text}\" is not a type of an invoice map: write date, amount or code"))
                })?),
            };
            check_path(&path, element, Shape::Value(value_type)).map_err(in_entry)?;
            Kind::Value(value_type)
        }
        _ => {
            let shape = if tag == "group" { Shape::Group } else { Shape::List };
            let end = check_path(&path, element, shape).map_err(in_entry)?;
            let holder = if tag == "group" { Holder::Group } else { Holder::List };
            let inner = read_entries(document, node, end, &full_name, holder)?;
            if inner.is_empty() {
                return Err(in_entry(format!("the <{tag}> has no entries: add a <value> inside it")));
            }
            if tag == "group" { Kind::Group(inner) } else { Kind::List(inner) }
        }
    };
    Ok(Entry { name: name.to_string(), full_name, path, required, kind })
}

/// Whether `name` may name an entry: a key a template can reach in a path such as `seller.vatId`.
fn is_entry_name(name: &str) -> bool {
    let mut characters = name.chars();
    characters.next().is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'))
}

/// What a path must end at.
#[derive(Clone, Copy)]
enum Shape {
    Value(Option<ValueType>),
    Group,
    List,
}

/// Checks `path` from `place` against the schema, and returns the element it ends at: each step an element the one
/// before may hold, each attribute one the element has, nothing that may repeat unless a test narrows it or the
/// path is a list's own last step, and an end that fits the entry.
fn check_path(path: &Path, place: &'static SchemaChild, shape: Shape) -> Result<&'static SchemaChild, String> {
    let mut element = place;
    // A test narrows the steps up to it to one: the steps before it are taken where the test holds.
    let last_test = path.steps.iter().rposition(|step| step.test.is_some());
    for (index, step) in path.steps.iter().enumerate() {
        let child = element.child(step.name.namespace, &step.name.local).ok_or_else(|| {
            format!("'{}' is not an element of {}. {}", step.name.written(), element.written(), holds(element))
        })?;
        if let Some(test) = &step.test {
            check_test(test, child, place)?;
        }
        let narrowed = last_test.is_some_and(|last| index <= last);
        let own_list_step = matches!(shape, Shape::List) && index + 1 == path.steps.len();
        if child.repeats && !narrowed && !own_list_step {
            return Err(format!(
                "{} may repeat in an invoice, so the path may find several: narrow it with a test in [ ], such as \
                {}[ram:TypeCode='VAT'], or make the entry a <list> of it",
                child.written(),
                child.written()
            ));
        }
        element = child;
    }
    if let Some(attribute) = &path.attribute
        && !element.schema_type().attributes.contains(&attribute.as_str())
    {
        return Err(format!("'@{attribute}' is not an attribute of {}. {}", element.written(), attributes_of(element)));
    }
    match shape {
        Shape::Value(value_type) => {
            if path.attribute.is_none() && !element.schema_type().text {
                return Err(format!(
                    "{} holds other elements, not a value: point the path at one of them. {}",
                    element.written(),
                    holds(element)
                ));
            }
            if value_type == Some(ValueType::Date)
                && (path.attribute.is_some() || !matches!(element.name, "DateTimeString" | "DateString"))
            {
                return Err(
                    "type=\"date\" reads a date, which an invoice writes in a udt:DateTimeString, a udt:DateString or a \
                    qdt:DateTimeString: point the path at it, such as rsm:ExchangedDocument/ram:IssueDateTime/udt:DateTimeString"
                        .to_string(),
                );
            }
        }
        Shape::Group | Shape::List => {
            let entry = if matches!(shape, Shape::Group) { "group" } else { "list" };
            if path.attribute.is_some() || path.steps.is_empty() {
                return Err(format!("a {entry}'s path ends at an element, not an attribute"));
            }
            if element.schema_type().text {
                return Err(format!(
                    "{} holds a value, not other elements: make the entry a <value>",
                    element.written()
                ));
            }
        }
    }
    Ok(element)
}

/// Checks a test of `element`: what it compares is an attribute the element has, or a child that holds a value; and a
/// value it compares with is found from `place` and is one.
fn check_test(test: &path::Test, element: &'static SchemaChild, place: &'static SchemaChild) -> Result<(), String> {
    match &test.subject {
        Subject::Attribute(attribute) => {
            if !element.schema_type().attributes.contains(&attribute.as_str()) {
                return Err(format!(
                    "the test [@{attribute}=…]: '@{attribute}' is not an attribute of {}. {}",
                    element.written(),
                    attributes_of(element)
                ));
            }
        }
        Subject::Child(name) => {
            let child = element.child(name.namespace, &name.local).ok_or_else(|| {
                format!("the test [{}=…]: it is not an element of {}. {}", name.written(), element.written(), holds(element))
            })?;
            if !child.schema_type().text {
                return Err(format!("the test [{}=…]: it holds other elements, not a value", name.written()));
            }
            if child.repeats {
                return Err(format!(
                    "the test [{}=…]: {} may repeat in an invoice, so the test would read only the first",
                    name.written(),
                    child.written()
                ));
            }
        }
    }
    if let Operand::Value(names) = &test.against {
        let written = names.iter().map(path::Name::written).collect::<Vec<_>>().join("/");
        let mut at = place;
        for name in names {
            at = at.child(name.namespace, &name.local).ok_or_else(|| {
                format!(
                    "the test compares with {written}, which is read from {}, the place of the entry's group, and {} is \
                    not an element there. {}",
                    place.written(),
                    name.written(),
                    holds(at)
                )
            })?;
            if at.repeats {
                return Err(format!("the test compares with {written}, and {} may repeat in an invoice", at.written()));
            }
        }
        if !at.schema_type().text {
            return Err(format!("the test compares with {written}, which holds other elements, not a value"));
        }
    }
    Ok(())
}

/// The elements `element` holds, for an error.
fn holds(element: &SchemaChild) -> String {
    let children = element.schema_type().children;
    if children.is_empty() {
        return format!("{} holds a value and no elements", element.written());
    }
    let names: Vec<String> = children.iter().map(SchemaChild::written).collect();
    format!("{} holds {}", element.written(), names.join(", "))
}

/// The attributes `element` has, for an error.
fn attributes_of(element: &SchemaChild) -> String {
    match element.schema_type().attributes {
        [] => format!("{} has no attributes", element.written()),
        attributes => format!("{} has @{}", element.written(), attributes.join(", @")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(entries: &str) -> Result<InvoiceMap, String> {
        parse(&format!("<cross-industry-invoice-map prefix=\"invoice\">\n{entries}\n</cross-industry-invoice-map>"))
    }

    const SELLER: &str = "rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty";
    const SETTLEMENT: &str = "rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeSettlement";
    const LINES: &str = "rsm:SupplyChainTradeTransaction/ram:IncludedSupplyChainTradeLineItem";

    fn refused(entries: &str) -> String {
        map(entries).expect_err("the map has an error")
    }

    #[test]
    fn a_map_reads_its_prefix_and_its_entries_with_their_full_names() {
        let map = map(&format!(
            r#"<value name="number" path="rsm:ExchangedDocument/ram:ID" required="true"/>
            <group name="seller" path="{SELLER}">
              <value name="name" path="ram:Name"/>
              <list name="contacts" path="ram:DefinedTradeContact">
                <value name="email" path="ram:EmailURIUniversalCommunication/ram:URIID"/>
              </list>
            </group>
            <list name="lines" path="{LINES}">
              <value name="unit" path="ram:SpecifiedLineTradeDelivery/ram:BilledQuantity/@unitCode" type="code"/>
            </list>"#
        ))
        .unwrap();
        assert_eq!(map.prefix, "invoice");
        assert_eq!(map.entries.len(), 3);
        assert!(map.entries[0].required);
        let Kind::Group(seller) = &map.entries[1].kind else { panic!("seller is a group") };
        let Kind::List(contacts) = &seller[1].kind else { panic!("contacts is a list") };
        assert_eq!(contacts[0].full_name, "seller.contacts.email");
        let Kind::List(lines) = &map.entries[2].kind else { panic!("lines is a list") };
        assert!(matches!(lines[0].kind, Kind::Value(Some(ValueType::Code))));
    }

    #[test]
    fn a_path_the_schema_lacks_is_an_error_that_names_the_element_and_its_parent() {
        let error = refused(&format!(r#"<group name="seller" path="{SELLER}"><value name="name" path="ram:Nmae"/></group>"#));
        assert!(error.starts_with("the invoice map, line 2: entry 'seller.name':"), "{error}");
        assert!(error.contains("'ram:Nmae' is not an element of ram:SellerTradeParty"), "{error}");
        assert!(error.contains("ram:SellerTradeParty holds ram:ID, ram:GlobalID, ram:Name"), "{error}");
        let error = refused(r#"<value name="unit" path="rsm:ExchangedDocument/ram:ID/@unitCode"/>"#);
        assert!(error.contains("'@unitCode' is not an attribute of ram:ID. ram:ID has @schemeID"), "{error}");
    }

    #[test]
    fn a_value_that_may_repeat_is_an_error_unless_a_test_narrows_it() {
        let repeating = format!(
            r#"<value name="vat" path="{SETTLEMENT}/ram:SpecifiedTradeSettlementHeaderMonetarySummation/ram:TaxTotalAmount" type="amount"/>"#
        );
        let error = refused(&repeating);
        assert!(error.contains("ram:TaxTotalAmount may repeat in an invoice"), "{error}");
        let narrowed = format!(
            r#"<group name="totals" path="{SETTLEMENT}">
                 <value name="vat" path="ram:SpecifiedTradeSettlementHeaderMonetarySummation/ram:TaxTotalAmount[@currencyID=ram:InvoiceCurrencyCode]"/>
               </group>"#
        );
        assert!(map(&narrowed).is_ok(), "{:?}", map(&narrowed));
        // A test narrows the steps before it too: a party has two tax registrations, one per scheme.
        let vat_id = format!(r#"<group name="seller" path="{SELLER}"><value name="vatId" path="ram:SpecifiedTaxRegistration/ram:ID[@schemeID='VA']"/></group>"#);
        assert!(map(&vat_id).is_ok(), "{:?}", map(&vat_id));
        // A list's own last step repeats, which is what makes it a list; the steps before it may not.
        assert!(map(&format!(r#"<list name="lines" path="{LINES}"><value name="id" path="ram:AssociatedDocumentLineDocument/ram:LineID"/></list>"#)).is_ok());
        let error = refused(&format!(r#"<group name="line" path="{LINES}/ram:AssociatedDocumentLineDocument"><value name="id" path="ram:LineID"/></group>"#));
        assert!(error.contains("ram:IncludedSupplyChainTradeLineItem may repeat"), "{error}");
    }

    #[test]
    fn the_value_a_test_compares_with_is_read_from_the_group_and_must_be_one_value() {
        let error = refused(&format!(
            r#"<group name="totals" path="{SETTLEMENT}">
                 <value name="vat" path="ram:SpecifiedTradeSettlementHeaderMonetarySummation/ram:TaxTotalAmount[@currencyID=ram:InvoiceCurrencyKode]"/>
               </group>"#
        ));
        assert!(error.contains("ram:InvoiceCurrencyKode is not an element there"), "{error}");
        let error = refused(&format!(
            r#"<group name="seller" path="{SELLER}"><value name="vatId" path="ram:SpecifiedTaxRegistration/ram:ID[@scheme='VA']"/></group>"#
        ));
        assert!(error.contains("'@scheme' is not an attribute of ram:ID"), "{error}");
        // A party may have several global identifiers, and a test reads one value.
        let error = refused(
            r#"<value name="seller" path="rsm:SupplyChainTradeTransaction/ram:ApplicableHeaderTradeAgreement/ram:SellerTradeParty[ram:GlobalID='4000001123452']/ram:Name"/>"#,
        );
        assert!(error.contains("the test [ram:GlobalID=…]: ram:GlobalID may repeat in an invoice"), "{error}");
    }

    #[test]
    fn an_end_that_doesnt_fit_the_entry_is_an_error() {
        let error = refused(&format!(r#"<value name="seller" path="{SELLER}"/>"#));
        assert!(error.contains("ram:SellerTradeParty holds other elements, not a value"), "{error}");
        let error = refused(r#"<group name="number" path="rsm:ExchangedDocument/ram:ID"><value name="x" path="@schemeID"/></group>"#);
        assert!(error.contains("ram:ID holds a value, not other elements: make the entry a <value>"), "{error}");
        let error = refused(r#"<value name="issued" path="rsm:ExchangedDocument/ram:ID" type="date"/>"#);
        assert!(error.contains("type=\"date\" reads a date"), "{error}");
    }

    #[test]
    fn the_form_of_the_map_is_checked_and_each_error_names_the_line_and_the_entry() {
        for (entries, says) in [
            (r#"<value path="rsm:ExchangedDocument/ram:ID"/>"#, "a <value> has no name"),
            (r#"<value name="number"/>"#, "<value> has no path"),
            (r#"<value name="seller.name" path="rsm:ExchangedDocument/ram:ID"/>"#, "a name is letters, digits"),
            (r#"<value name="number" path="rsm:ExchangedDocument/ram:ID" kind="x"/>"#, "<value> has no attribute kind"),
            (r#"<value name="number" path="rsm:ExchangedDocument/ram:ID" type="text"/>"#, "type=\"text\" is not a type"),
            (r#"<value name="number" path="rsm:ExchangedDocument/ram:ID" required="yes"/>"#, "required=\"yes\" is not true or false"),
            (r#"<field name="number" path="rsm:ExchangedDocument/ram:ID"/>"#, "<field> is not an entry of an invoice map"),
            (r#"<value name="n" path="rsm:ExchangedDocument/ram:ID"/><value name="n" path="rsm:ExchangedDocument/ram:TypeCode"/>"#, "two entries are named 'n'"),
            (r#"<value name="n" path="rsm:ExchangedDocument/ran:ID"/>"#, "'ran:' is not a prefix"),
            (&format!(r#"<group name="seller" path="{SELLER}"/>"#), "the <group> has no entries"),
            (r#"some text"#, "text 'some text' outside an entry"),
        ] {
            let error = refused(entries);
            assert!(error.starts_with("the invoice map, line "), "{entries}: {error}");
            assert!(error.contains(says), "{entries}: {error}");
        }
    }

    #[test]
    fn a_list_holds_values_only() {
        let error = refused(&format!(
            r#"<list name="lines" path="{LINES}">
                 <list name="charges" path="ram:SpecifiedLineTradeSettlement/ram:SpecifiedTradeAllowanceCharge">
                   <value name="amount" path="ram:ActualAmount"/>
                 </list>
               </list>"#
        ));
        assert!(error.contains("entry 'lines.charges': a <list> can't be inside the list 'lines'"), "{error}");
    }

    #[test]
    fn the_root_and_its_prefix_are_checked() {
        let error = parse(r#"<map prefix="invoice"><value name="n" path="rsm:ExchangedDocument/ram:ID"/></map>"#).unwrap_err();
        assert!(error.contains("the invoice map's root is <map>"), "{error}");
        let error = parse(r#"<cross-industry-invoice-map><value name="n" path="rsm:ExchangedDocument/ram:ID"/></cross-industry-invoice-map>"#).unwrap_err();
        assert!(error.contains("has no prefix"), "{error}");
        let error = parse(r#"<cross-industry-invoice-map prefix="in voice"><value name="n" path="rsm:ExchangedDocument/ram:ID"/></cross-industry-invoice-map>"#).unwrap_err();
        assert!(error.contains("prefix 'in voice' is not a name"), "{error}");
        let german = parse(r#"<cross-industry-invoice-map prefix="rechnung"><value name="nummer" path="rsm:ExchangedDocument/ram:ID"/></cross-industry-invoice-map>"#).unwrap();
        assert_eq!(german.prefix, "rechnung");
        let error = parse(r#"<cross-industry-invoice-map prefix="invoice"/>"#).unwrap_err();
        assert!(error.contains("has no entries"), "{error}");
    }

    #[test]
    fn a_map_with_a_dtd_is_refused() {
        let error = parse(
            r#"<!DOCTYPE m [<!ENTITY a "aaaa">]><cross-industry-invoice-map prefix="invoice"><value name="n" path="rsm:ExchangedDocument/ram:ID"/></cross-industry-invoice-map>"#,
        )
        .unwrap_err();
        assert!(error.contains("the invoice map has a DTD"), "{error}");
    }
}
