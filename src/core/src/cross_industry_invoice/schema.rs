//! The elements of a Cross Industry Invoice: which element may sit inside which, whether it may repeat, whether it
//! holds a value, and its attributes. The table is made at build from FeRD's EXTENDED schema (`build_cii_schema.rs`),
//! which every profile of Factur-X / ZUGFeRD is a subset of, so a path it allows is one some invoice may have.

/// The namespaces of a Cross Industry Invoice, each with the prefix the spec and an invoice map write it with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Namespace {
    Rsm,
    Ram,
    Udt,
    Qdt,
}

impl Namespace {
    const ALL: [Namespace; 4] = [Namespace::Rsm, Namespace::Ram, Namespace::Udt, Namespace::Qdt];

    /// The prefix a map writes the namespace with: `ram`.
    pub(crate) fn prefix(self) -> &'static str {
        match self {
            Namespace::Rsm => "rsm",
            Namespace::Ram => "ram",
            Namespace::Udt => "udt",
            Namespace::Qdt => "qdt",
        }
    }

    /// The namespace's URI, which an invoice may give any prefix.
    pub(crate) fn uri(self) -> &'static str {
        match self {
            Namespace::Rsm => "urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100",
            Namespace::Ram => "urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100",
            Namespace::Udt => "urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100",
            Namespace::Qdt => "urn:un:unece:uncefact:data:standard:QualifiedDataType:100",
        }
    }

    /// The namespace a map's prefix names.
    pub(crate) fn from_prefix(prefix: &str) -> Option<Namespace> {
        Self::ALL.into_iter().find(|namespace| namespace.prefix() == prefix)
    }

    /// The prefixes a map may write, for its errors: `rsm:, ram:, udt: and qdt:`.
    pub(crate) fn prefixes() -> &'static str {
        "rsm:, ram:, udt: and qdt:"
    }
}

/// A type of the schema: the elements it holds, or its value and attributes.
#[derive(Debug)]
pub(crate) struct SchemaType {
    /// Whether an element of the type holds a value, rather than other elements.
    pub text:       bool,
    pub attributes: &'static [&'static str],
    pub children:   &'static [SchemaChild],
}

/// An element a type holds: its name, its type in `TYPES`, and whether it may repeat.
#[derive(Debug)]
pub(crate) struct SchemaChild {
    pub namespace: Namespace,
    pub name:      &'static str,
    pub ty:        usize,
    pub repeats:   bool,
}

impl SchemaChild {
    /// The type of the element.
    pub(crate) fn schema_type(&self) -> &'static SchemaType {
        &TYPES[self.ty]
    }

    /// The element of this name that this element may hold.
    pub(crate) fn child(&self, namespace: Namespace, name: &str) -> Option<&'static SchemaChild> {
        self.schema_type().children.iter().find(|child| child.namespace == namespace && child.name == name)
    }

    /// The element as a map writes it: `ram:SellerTradeParty`.
    pub(crate) fn written(&self) -> String {
        format!("{}:{}", self.namespace.prefix(), self.name)
    }
}

include!(concat!(env!("OUT_DIR"), "/cii_schema.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    fn at(path: &[&str]) -> &'static SchemaChild {
        let mut element = &ROOT;
        for step in path {
            let (prefix, name) = step.split_once(':').expect("a prefixed name");
            let namespace = Namespace::from_prefix(prefix).expect("a known prefix");
            element = element.child(namespace, name).unwrap_or_else(|| panic!("{step} is not in {}", element.name));
        }
        element
    }

    #[test]
    fn the_table_holds_the_elements_of_an_invoice_from_its_root() {
        assert_eq!(ROOT.written(), "rsm:CrossIndustryInvoice");
        let seller = at(&[
            "rsm:SupplyChainTradeTransaction",
            "ram:ApplicableHeaderTradeAgreement",
            "ram:SellerTradeParty",
        ]);
        assert!(!seller.repeats && !seller.schema_type().text);
        assert!(seller.child(Namespace::Ram, "Name").is_some_and(|name| name.schema_type().text));
        assert!(seller.child(Namespace::Ram, "Nmae").is_none());
    }

    #[test]
    fn an_element_says_whether_it_repeats_and_which_attributes_it_has() {
        let lines = at(&["rsm:SupplyChainTradeTransaction", "ram:IncludedSupplyChainTradeLineItem"]);
        assert!(lines.repeats);
        let quantity = at(&[
            "rsm:SupplyChainTradeTransaction",
            "ram:IncludedSupplyChainTradeLineItem",
            "ram:SpecifiedLineTradeDelivery",
            "ram:BilledQuantity",
        ]);
        assert_eq!(quantity.schema_type().attributes, ["unitCode"]);
        let date = at(&["rsm:ExchangedDocument", "ram:IssueDateTime", "udt:DateTimeString"]);
        assert!(date.schema_type().text);
        assert_eq!(date.schema_type().attributes, ["format"]);
    }

    #[test]
    fn the_tax_totals_may_repeat_twice_and_a_party_two_tax_registrations() {
        let totals = at(&[
            "rsm:SupplyChainTradeTransaction",
            "ram:ApplicableHeaderTradeSettlement",
            "ram:SpecifiedTradeSettlementHeaderMonetarySummation",
            "ram:TaxTotalAmount",
        ]);
        assert!(totals.repeats);
        assert_eq!(totals.schema_type().attributes, ["currencyID"]);
        let registrations = at(&[
            "rsm:SupplyChainTradeTransaction",
            "ram:ApplicableHeaderTradeAgreement",
            "ram:SellerTradeParty",
            "ram:SpecifiedTaxRegistration",
        ]);
        assert!(registrations.repeats);
    }
}
