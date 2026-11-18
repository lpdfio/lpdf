//! The paths of an invoice map: where a value, a group or a list sits in the invoice, from the place of the entry
//! around it. They are written in the spelling of XPath but are not XPath, and their form is closed:
//!
//! - an element by its place: `ram:SellerTradeParty/ram:Name`;
//! - an attribute at the end: `ram:BilledQuantity/@unitCode`;
//! - on any element, one test in `[ ]` on one of its attributes or on the text of one of its child elements, against
//!   fixed text in quotes or against another value of the invoice, a path of elements read from the place of the group
//!   around the entry: `ram:ID[@schemeID='VA']`, `ram:ApplicableTradeTax[ram:TypeCode='VAT']`,
//!   `ram:TaxTotalAmount[@currencyID=ram:InvoiceCurrencyCode]`.
//!
//! A path finds a value and never computes one: anything else is an error.

use super::schema::Namespace;

/// An element's name, `ram:Name`, with its namespace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Name {
    pub namespace: Namespace,
    pub local:     String,
}

impl Name {
    /// The name as a map writes it.
    pub(crate) fn written(&self) -> String {
        format!("{}:{}", self.namespace.prefix(), self.local)
    }
}

/// One element of a path, with its test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Step {
    pub name: Name,
    pub test: Option<Test>,
}

/// The test of a step: what of the element is compared, and with what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Test {
    pub subject: Subject,
    pub against: Operand,
}

/// What a test compares: an attribute of the element, or the text of one of its child elements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Subject {
    Attribute(String),
    Child(Name),
}

/// What a test compares with: fixed text, or the value at a path of elements from the place of the group around the
/// entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Operand {
    Text(String),
    Value(Vec<Name>),
}

/// A path: its elements, and the attribute it ends with, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Path {
    pub steps:     Vec<Step>,
    pub attribute: Option<String>,
}

/// What a path may say, for its errors.
const FORM: &str = "a path is elements such as ram:SellerTradeParty/ram:Name, may end with an attribute such as \
    /@unitCode, and may test an element in [ ] against fixed text or another value, such as ram:ID[@schemeID='VA'], \
    ram:ApplicableTradeTax[ram:TypeCode='VAT'] or ram:TaxTotalAmount[@currencyID=ram:InvoiceCurrencyCode]";

/// Reads a path, or says what in it is outside the form.
pub(crate) fn parse(text: &str) -> Result<Path, String> {
    if text.is_empty() {
        return Err(format!("the path is empty: {FORM}"));
    }
    let parts = split_steps(text)?;
    let mut steps = Vec::with_capacity(parts.len());
    let mut attribute = None;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            return Err(format!("'{text}' has an empty step: {FORM}"));
        }
        if let Some(name) = part.strip_prefix('@') {
            if index + 1 != parts.len() {
                return Err(format!("'{text}': an attribute such as @{name} goes only at the end of a path"));
            }
            attribute = Some(attribute_name(name, text)?);
            continue;
        }
        steps.push(parse_step(part, text)?);
    }
    Ok(Path { steps, attribute })
}

/// The steps of a path, split at each `/` outside `[ ]` and quotes.
fn split_steps(text: &str) -> Result<Vec<&str>, String> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    let mut quote = None;
    for (at, character) in text.char_indices() {
        match (quote, character) {
            (Some(open), c) if c == open => quote = None,
            (Some(_), _) => {}
            (None, '\'' | '"') => quote = Some(character),
            (None, '[') => depth += 1,
            (None, ']') => depth -= 1,
            (None, '/') if depth == 0 => {
                parts.push(&text[start..at]);
                start = at + 1;
            }
            _ => {}
        }
        if depth < 0 {
            return Err(format!("'{text}' closes a ] it didn't open: {FORM}"));
        }
    }
    if quote.is_some() {
        return Err(format!("'{text}' has a quote that isn't closed"));
    }
    if depth != 0 {
        return Err(format!("'{text}' has a [ that isn't closed"));
    }
    parts.push(&text[start..]);
    Ok(parts)
}

/// One step: a name, then a test in `[ ]`, and nothing after it.
fn parse_step(part: &str, path: &str) -> Result<Step, String> {
    let Some(open) = part.find('[') else {
        return Ok(Step { name: element_name(part, path)?, test: None });
    };
    let name = element_name(&part[..open], path)?;
    let Some(inner) = part[open + 1..].strip_suffix(']') else {
        return Err(format!("'{path}': nothing may follow the ] of a test: {FORM}"));
    };
    Ok(Step { name, test: Some(parse_test(inner, path)?) })
}

/// A test: `@attribute=` or `prefix:Child=`, then a quoted text or a path of elements.
fn parse_test(inner: &str, path: &str) -> Result<Test, String> {
    let Some((subject, against)) = inner.split_once('=') else {
        return Err(format!("'{path}': [{inner}] is not a test: [ ] holds one comparison with =, such as [@schemeID='VA']"));
    };
    let subject = match subject.strip_prefix('@') {
        Some(attribute) => Subject::Attribute(attribute_name(attribute, path)?),
        None => Subject::Child(element_name(subject, path)?),
    };
    let quoted = ['\'', '"'].into_iter().find(|quote| against.starts_with(*quote));
    let against = match quoted {
        Some(quote) => match against[1..].strip_suffix(quote) {
            Some(text) if !text.contains(quote) => Operand::Text(text.to_string()),
            _ => return Err(format!("'{path}': [{inner}] compares with a text whose quotes don't match")),
        },
        None => Operand::Value(
            against.split('/').map(|name| element_name(name, path)).collect::<Result<Vec<_>, _>>()?,
        ),
    };
    Ok(Test { subject, against })
}

/// An element's name: a known prefix, a colon and a local name.
fn element_name(text: &str, path: &str) -> Result<Name, String> {
    let Some((prefix, local)) = text.split_once(':') else {
        return Err(format!(
            "'{path}': '{text}' is not an element's name, which has a prefix, such as ram:Name. The prefixes are {}",
            Namespace::prefixes()
        ));
    };
    if !is_local_name(prefix) || !is_local_name(local) {
        return Err(format!("'{path}': '{text}' is not an element's name, such as ram:Name: {FORM}"));
    }
    let namespace = Namespace::from_prefix(prefix).ok_or_else(|| {
        format!("'{path}': '{prefix}:' is not a prefix of a Cross Industry Invoice. The prefixes are {}", Namespace::prefixes())
    })?;
    Ok(Name { namespace, local: local.to_string() })
}

/// An attribute's name, which in an invoice has no prefix: `schemeID`.
fn attribute_name(text: &str, path: &str) -> Result<String, String> {
    if is_local_name(text) {
        Ok(text.to_string())
    } else {
        Err(format!("'{path}': '@{text}' is not an attribute's name, such as @schemeID"))
    }
}

/// Whether `text` is a name as XML writes one without a prefix.
fn is_local_name(text: &str) -> bool {
    let mut characters = text.chars();
    characters.next().is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && characters.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn name(namespace: Namespace, local: &str) -> Name {
        Name { namespace, local: local.to_string() }
    }

    fn step(local: &str) -> Step {
        Step { name: name(Namespace::Ram, local), test: None }
    }

    #[test]
    fn a_path_is_its_elements_and_the_attribute_it_ends_with() {
        assert_eq!(
            parse("ram:SpecifiedLineTradeDelivery/ram:BilledQuantity/@unitCode"),
            Ok(Path { steps: vec![step("SpecifiedLineTradeDelivery"), step("BilledQuantity")], attribute: Some("unitCode".into()) })
        );
        assert_eq!(
            parse("rsm:ExchangedDocument/ram:IssueDateTime/udt:DateTimeString").map(|p| p.steps.len()),
            Ok(3)
        );
        assert_eq!(parse("@unitCode"), Ok(Path { steps: vec![], attribute: Some("unitCode".into()) }));
    }

    #[test]
    fn a_test_compares_an_attribute_or_a_child_with_a_text_or_another_value() {
        let vat_id = parse("ram:SpecifiedTaxRegistration/ram:ID[@schemeID='VA']").unwrap();
        assert_eq!(
            vat_id.steps[1].test,
            Some(Test { subject: Subject::Attribute("schemeID".into()), against: Operand::Text("VA".into()) })
        );
        let vat = parse(r#"ram:ApplicableTradeTax[ram:TypeCode="VAT"]/ram:RateApplicablePercent"#).unwrap();
        assert_eq!(
            vat.steps[0].test,
            Some(Test { subject: Subject::Child(name(Namespace::Ram, "TypeCode")), against: Operand::Text("VAT".into()) })
        );
        let total = parse("ram:TaxTotalAmount[@currencyID=ram:InvoiceCurrencyCode]").unwrap();
        assert_eq!(
            total.steps[0].test,
            Some(Test {
                subject: Subject::Attribute("currencyID".into()),
                against: Operand::Value(vec![name(Namespace::Ram, "InvoiceCurrencyCode")]),
            })
        );
        let slash_in_text = parse("ram:IncludedNote[ram:Content='a/b']/ram:SubjectCode").unwrap();
        assert_eq!(slash_in_text.steps.len(), 2);
    }

    #[test]
    fn what_is_outside_the_form_is_an_error_that_says_what() {
        for (path, says) in [
            ("", "the path is empty"),
            ("ram:A//ram:B", "has an empty step"),
            ("ram:A/", "has an empty step"),
            ("Name", "has a prefix, such as ram:Name"),
            ("ran:Name", "'ran:' is not a prefix of a Cross Industry Invoice"),
            ("@unitCode/ram:A", "goes only at the end of a path"),
            ("ram:A[@b='c']x", "nothing may follow the ]"),
            ("ram:A[@b]", "is not a test"),
            ("ram:A[@b='c\"]", "a quote that isn't closed"),
            ("ram:A[@b='c'", "a [ that isn't closed"),
            ("ram:A]", "closes a ] it didn't open"),
            ("ram:A[@b=ram:C[@d='e']]", "is not an element's name"),
            ("ram:A[@b=count(ram:C)]", "is not an element's name"),
            ("ram:A/text()", "is not an element's name"),
            ("ram:A[1]", "is not a test"),
            ("ram:*", "is not an element's name"),
            ("ram:A/@b c", "is not an attribute's name"),
        ] {
            let error = parse(path).unwrap_err();
            assert!(error.contains(says), "{path}: {error}");
        }
    }
}
