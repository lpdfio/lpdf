//! Reads FeRD's EXTENDED schema of a Cross Industry Invoice (`vendor/factur-x-zugferd/`) and writes, into the build
//! output, which element may sit inside which, whether it may repeat, whether it holds a value, and its attributes:
//! `cii_schema.rs`, which `cross_industry_invoice/schema.rs` includes. The build scripts of the core and of the WASI
//! binary call it, so nothing generated is committed.
//!
//! The schema uses few of XSD's forms, and this reads those only: named and anonymous complex types made of a
//! sequence or a choice of elements, or of simple content with attributes; simple types; and global elements. Any
//! other form stops the build, so a new version of the schema that uses one is read again first.

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::path::Path;

const XS: &str = "http://www.w3.org/2001/XMLSchema";

/// The namespaces of a CII invoice, as the variants of `schema::Namespace` that the generated code names.
const NAMESPACES: [(&str, &str); 4] = [
    ("urn:un:unece:uncefact:data:standard:CrossIndustryInvoice:100", "Rsm"),
    ("urn:un:unece:uncefact:data:standard:ReusableAggregateBusinessInformationEntity:100", "Ram"),
    ("urn:un:unece:uncefact:data:standard:UnqualifiedDataType:100", "Udt"),
    ("urn:un:unece:uncefact:data:standard:QualifiedDataType:100", "Qdt"),
];

/// A name in a namespace: `(namespace URI, local name)`.
type QName = (String, String);

/// A child element of a complex type.
struct Child {
    name:    QName,
    ty:      TypeRef,
    repeats: bool,
}

/// What an element's type is.
enum TypeRef {
    /// A type of XSD itself, or a simple type: the element holds a value and has no attributes.
    Simple,
    /// A complex type, by name.
    Named(QName),
    /// The complex type written inside the element, under the name given to it here.
    Anonymous(QName),
}

/// A complex type: its child elements, or its value and attributes.
#[derive(Default)]
struct Complex {
    children:   Vec<Child>,
    text:       bool,
    attributes: Vec<String>,
    /// The complex type its simple content extends, whose attributes it has too.
    base:       Option<QName>,
}

/// The schema's types and global elements, as read.
#[derive(Default)]
struct Schema {
    complex:  BTreeMap<QName, Complex>,
    simple:   Vec<QName>,
    elements: Vec<(QName, QName)>,
}

/// Reads the `.xsd` files of `folder` and writes the table to `out`.
pub fn generate(folder: &Path, out: &Path) {
    let mut schema = Schema::default();
    let mut files: Vec<_> = std::fs::read_dir(folder)
        .unwrap_or_else(|e| panic!("can't read {}: {e}", folder.display()))
        .map(|entry| entry.expect("a folder entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "xsd"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "{} has no .xsd file", folder.display());
    for file in &files {
        let text = std::fs::read_to_string(file).unwrap_or_else(|e| panic!("can't read {}: {e}", file.display()));
        let document = roxmltree::Document::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        read_schema(&document, &mut schema, &file.display().to_string());
    }
    std::fs::write(out, write_table(&schema)).unwrap_or_else(|e| panic!("can't write {}: {e}", out.display()));
}

fn is_xs(node: &roxmltree::Node, local: &str) -> bool {
    node.is_element() && node.tag_name().namespace() == Some(XS) && node.tag_name().name() == local
}

/// The namespace and local name a `prefix:local` attribute value names, in the scope of `node`.
fn resolve(node: &roxmltree::Node, value: &str) -> QName {
    let (prefix, local) = value.split_once(':').map_or((None, value), |(p, l)| (Some(p), l));
    let namespace = node
        .lookup_namespace_uri(prefix)
        .unwrap_or_else(|| panic!("the prefix of '{value}' names no namespace"));
    (namespace.to_string(), local.to_string())
}

fn read_schema(document: &roxmltree::Document, schema: &mut Schema, file: &str) {
    let root = document.root_element();
    assert!(is_xs(&root, "schema"), "{file}: the root is not xs:schema");
    let target = root.attribute("targetNamespace").unwrap_or_else(|| panic!("{file}: no targetNamespace")).to_string();
    for node in root.children().filter(roxmltree::Node::is_element) {
        let name = || node.attribute("name").unwrap_or_else(|| panic!("{file}: a {} has no name", node.tag_name().name()));
        match node.tag_name().name() {
            "import" | "annotation" => {}
            "element" => {
                let ty = node.attribute("type").unwrap_or_else(|| panic!("{file}: element {} has no type", name()));
                schema.elements.push(((target.clone(), name().to_string()), resolve(&node, ty)));
            }
            "complexType" => {
                let qname = (target.clone(), name().to_string());
                read_complex(&node, &qname, &target, schema, file);
            }
            "simpleType" => schema.simple.push((target.clone(), name().to_string())),
            other => panic!("{file}: xs:{other} at the top of the schema is a form this build doesn't read"),
        }
    }
}

/// Reads a complex type, named `qname`, and the anonymous types inside it.
fn read_complex(node: &roxmltree::Node, qname: &QName, target: &str, schema: &mut Schema, file: &str) {
    let mut complex = Complex::default();
    for part in node.children().filter(roxmltree::Node::is_element) {
        match part.tag_name().name() {
            "sequence" | "choice" => read_particles(&part, qname, target, &mut complex, schema, file),
            "simpleContent" => {
                let extension = part
                    .children()
                    .find(|n| is_xs(n, "extension"))
                    .unwrap_or_else(|| panic!("{file}: {} has simple content that is not an extension", qname.1));
                complex.text = true;
                let base = resolve(&extension, extension.attribute("base").expect("an extension's base"));
                if base.0 != XS {
                    complex.base = Some(base);
                }
                for attribute in extension.children().filter(roxmltree::Node::is_element) {
                    assert!(is_xs(&attribute, "attribute"), "{file}: {} extends with a form this build doesn't read", qname.1);
                    complex.attributes.push(attribute.attribute("name").expect("an attribute's name").to_string());
                }
            }
            "annotation" => {}
            other => panic!("{file}: xs:{other} in {} is a form this build doesn't read", qname.1),
        }
    }
    schema.complex.insert(qname.clone(), complex);
}

/// Reads the elements of a sequence or a choice, and of those inside it.
fn read_particles(
    node: &roxmltree::Node, owner: &QName, target: &str, complex: &mut Complex, schema: &mut Schema, file: &str,
) {
    for particle in node.children().filter(roxmltree::Node::is_element) {
        match particle.tag_name().name() {
            "sequence" | "choice" => read_particles(&particle, owner, target, complex, schema, file),
            "element" => {
                let local = particle.attribute("name").unwrap_or_else(|| panic!("{file}: an element in {} has no name", owner.1));
                let repeats = match particle.attribute("maxOccurs") {
                    None => false,
                    Some("unbounded") => true,
                    Some(n) => n.parse::<u32>().unwrap_or_else(|_| panic!("{file}: maxOccurs '{n}'")) > 1,
                };
                let ty = if let Some(ty) = particle.attribute("type") {
                    let ty = resolve(&particle, ty);
                    if ty.0 == XS { TypeRef::Simple } else { TypeRef::Named(ty) }
                } else if let Some(inner) = particle.children().find(|n| is_xs(n, "complexType")) {
                    let anonymous = (target.to_string(), format!("{}/{local}", owner.1));
                    read_complex(&inner, &anonymous, target, schema, file);
                    TypeRef::Anonymous(anonymous)
                } else {
                    panic!("{file}: element {local} of {} has no type", owner.1);
                };
                complex.children.push(Child { name: (target.to_string(), local.to_string()), ty, repeats });
            }
            "annotation" => {}
            other => panic!("{file}: xs:{other} in {} is a form this build doesn't read", owner.1),
        }
    }
}

/// The Rust source of the table: one `SchemaType` for each complex type, in the order of their names, then one for a
/// value with no attributes, and the root element.
fn write_table(schema: &Schema) -> String {
    let namespace = |uri: &str| {
        NAMESPACES.iter().find(|(u, _)| *u == uri).map(|(_, v)| *v).unwrap_or_else(|| panic!("namespace {uri} is not one of CII's"))
    };
    let index: HashMap<&QName, usize> = schema.complex.keys().enumerate().map(|(i, q)| (q, i)).collect();
    let plain = schema.complex.len();
    let type_of = |ty: &TypeRef| -> usize {
        match ty {
            TypeRef::Simple => plain,
            TypeRef::Named(q) | TypeRef::Anonymous(q) => match index.get(q) {
                Some(&i) => i,
                None if schema.simple.contains(q) => plain,
                None => panic!("type {}:{} is not in the schema", q.0, q.1),
            },
        }
    };
    let attributes_of = |complex: &Complex| -> Vec<String> {
        let mut attributes = complex.attributes.clone();
        let mut base = complex.base.as_ref();
        while let Some(q) = base {
            let Some(parent) = schema.complex.get(q) else { break };
            attributes.extend(parent.attributes.iter().cloned());
            base = parent.base.as_ref();
        }
        attributes
    };

    let mut src = String::from("// Generated by build_cii_schema.rs from vendor/factur-x-zugferd/: do not edit.\n\n");
    let _ = writeln!(src, "pub(crate) static TYPES: [SchemaType; {}] = [", plain + 1);
    for complex in schema.complex.values() {
        let _ = writeln!(src, "    SchemaType {{ text: {}, attributes: &{:?}, children: &[", complex.text, attributes_of(complex));
        for child in &complex.children {
            let _ = writeln!(
                src,
                "        SchemaChild {{ namespace: Namespace::{}, name: {:?}, ty: {}, repeats: {} }},",
                namespace(&child.name.0), child.name.1, type_of(&child.ty), child.repeats
            );
        }
        src.push_str("    ] },\n");
    }
    src.push_str("    SchemaType { text: true, attributes: &[], children: &[] },\n];\n\n");
    let (root, ty) = schema
        .elements
        .iter()
        .find(|((_, local), _)| local == "CrossIndustryInvoice")
        .expect("the schema has no CrossIndustryInvoice element");
    let _ = writeln!(
        src,
        "pub(crate) const ROOT: SchemaChild = SchemaChild {{ namespace: Namespace::{}, name: {:?}, ty: {}, repeats: false }};",
        namespace(&root.0), root.1, type_of(&TypeRef::Named(ty.clone()))
    );
    src
}
