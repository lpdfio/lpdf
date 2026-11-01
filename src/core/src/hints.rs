//! The hint tables of a linearized file (ISO 32000-1, F.4): where each page is and how long, which objects the pages
//! share, and where the outline is, so that a viewer that fetches the file by ranges can ask for one page.
//!
//! Every offset in them is the one the object would have if the hint stream were not in the file: a reader adds the
//! stream's length to an offset at or past it (F.4, and `adjusted_offset` in qpdf).
//!
//! Each count of bits is at least 1, where 0 would do when every page has as many objects or the same length:
//! PDFium, the reader of Chrome, drops the hints of a page offset table with a count of 0, and so loads the whole file
//! first. The tables follow qpdf otherwise, whose `--check-linearization` reads them back against the file: no
//! content stream offsets, each page's content taken as the whole page (as Acrobat writes them, implementation note
//! 127), and one object in each shared group.

/// A page of the page offset hint table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PageHint {
    /// How many objects the page has, its page object first, numbered one after the other.
    pub(crate) objects: u32,
    /// The length of those objects in the file, in bytes.
    pub(crate) length:  u32,
    /// The shared objects the page uses, by their place in the shared object table. Page 1 has none: its section holds
    /// what it shares.
    pub(crate) shared:  Vec<u32>,
}

/// What the hint tables say.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Hints {
    /// Where the page object of page 1 is.
    pub(crate) first_page_offset:   u32,
    pub(crate) pages:               Vec<PageHint>,
    /// The length of each object of the shared object table: those of page 1's section, then those of the section of
    /// the objects the other pages share.
    pub(crate) shared_lengths:      Vec<u32>,
    /// How many of `shared_lengths` are page 1's.
    pub(crate) first_page_shared:   u32,
    /// The number of the first object of the shared objects' section, and where it is. When the section is empty, the
    /// object where it would start: PDFium refuses a table whose number or offset is 0.
    pub(crate) first_shared_object: u32,
    pub(crate) first_shared_offset: u32,
    pub(crate) outline:             Option<OutlineHint>,
}

/// The outline: its first object, the outline dictionary, where it is, and how many objects and bytes it takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OutlineHint {
    pub(crate) first_object: u32,
    pub(crate) offset:       u32,
    pub(crate) objects:      u32,
    pub(crate) length:       u32,
}

/// The data of the hint stream: the tables, one after the other.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HintStream {
    pub(crate) data:    Vec<u8>,
    /// Where the shared object table starts in `data`: `/S` of the stream.
    pub(crate) shared:  usize,
    /// Where the outline table starts in `data`: `/O`, when there is an outline.
    pub(crate) outline: Option<usize>,
}

impl Hints {
    /// Writes the tables.
    pub(crate) fn encode(&self) -> HintStream {
        let mut bits = BitWriter::default();
        self.write_page_offsets(&mut bits);
        let shared = bits.bytes.len();
        self.write_shared_objects(&mut bits);
        let outline = self.outline.as_ref().map(|outline| {
            let at = bits.bytes.len();
            bits.write(outline.first_object, 32);
            bits.write(outline.offset, 32);
            bits.write(outline.objects, 32);
            bits.write(outline.length, 32);
            at
        });
        HintStream { data: bits.bytes, shared, outline }
    }

    /// The page offset hint table (F.4.1): its header, then each item for every page in turn, each item from a byte.
    fn write_page_offsets(&self, bits: &mut BitWriter) {
        let objects = self.pages.iter().map(|page| page.objects);
        let lengths = self.pages.iter().map(|page| page.length);
        let least_objects = objects.clone().min().unwrap_or(0);
        let least_length = lengths.clone().min().unwrap_or(0);
        let objects_bits = width(objects.max().unwrap_or(0) - least_objects);
        let length_bits = width(lengths.max().unwrap_or(0) - least_length);
        let shared_count_bits = width(self.pages.iter().map(|page| page.shared.len() as u32).max().unwrap_or(0));
        let shared_id_bits = width(self.shared_lengths.len() as u32);

        bits.write(least_objects, 32);              // 1  the least number of objects in a page
        bits.write(self.first_page_offset, 32);     // 2  where page 1's page object is
        bits.write(objects_bits, 16);               // 3
        bits.write(least_length, 32);               // 4  the least length of a page
        bits.write(length_bits, 16);                // 5
        bits.write(0, 32);                          // 6  the least offset of a content stream in its page
        bits.write(0, 16);                          // 7
        bits.write(least_length, 32);               // 8  the least length of a content stream: the page's
        bits.write(length_bits, 16);                // 9
        bits.write(shared_count_bits, 16);          // 10
        bits.write(shared_id_bits, 16);             // 11
        bits.write(0, 16);                          // 12 no bits for where a shared object is first used
        bits.write(1, 16);                          // 13 the denominator of that position

        bits.rows(self.pages.iter().map(|page| (page.objects - least_objects, objects_bits)));
        bits.rows(self.pages.iter().map(|page| (page.length - least_length, length_bits)));
        bits.rows(self.pages.iter().map(|page| (page.shared.len() as u32, shared_count_bits)));
        bits.rows(self.pages.iter().flat_map(|page| page.shared.iter().map(|&id| (id, shared_id_bits))));
        // The numerators of where each shared object is first used, and the content streams' offsets: no bits.
        bits.rows(self.pages.iter().map(|page| (page.length - least_length, length_bits)));
    }

    /// The shared object hint table (F.4.2): its header, then the length of each group, a flag for each that it has no
    /// MD5 signature, and the number of objects in each less one, which is 0, in 0 bits, as Acrobat writes it.
    fn write_shared_objects(&self, bits: &mut BitWriter) {
        let least_length = self.shared_lengths.iter().copied().min().unwrap_or(0);
        let length_bits = width(self.shared_lengths.iter().copied().max().unwrap_or(0) - least_length);

        bits.write(self.first_shared_object, 32);          // 1
        bits.write(self.first_shared_offset, 32);          // 2
        bits.write(self.first_page_shared, 32);            // 3
        bits.write(self.shared_lengths.len() as u32, 32);  // 4
        bits.write(0, 16);                                 // 5  one object in each group
        bits.write(least_length, 32);                      // 6
        bits.write(length_bits, 16);                       // 7

        bits.rows(self.shared_lengths.iter().map(|&length| (length - least_length, length_bits)));
        bits.rows(self.shared_lengths.iter().map(|_| (0, 1)));
    }
}

/// The bits to write any number up to `max` in, and at least 1.
fn width(max: u32) -> u32 {
    (u32::BITS - max.leading_zeros()).max(1)
}

/// Writes numbers of any width up to 32 bits, the high bit first.
#[derive(Default)]
struct BitWriter {
    bytes: Vec<u8>,
    /// How many bits of the last byte are written; 0 when it is full or there is none.
    used:  u32,
}

impl BitWriter {
    fn write(&mut self, value: u32, width: u32) {
        for bit in (0..width).rev() {
            if self.used == 0 {
                self.bytes.push(0);
            }
            if (value >> bit) & 1 == 1 {
                let last = self.bytes.len() - 1;
                self.bytes[last] |= 0x80 >> self.used;
            }
            self.used = (self.used + 1) % 8;
        }
    }

    /// Writes one item for each page or group, then goes on from the next byte.
    fn rows(&mut self, values: impl Iterator<Item = (u32, u32)>) {
        for (value, width) in values {
            self.write(value, width);
        }
        self.used = 0;
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads numbers back, the high bit first, as a viewer reads the tables.
    struct BitReader<'a> {
        bytes: &'a [u8],
        bit:   usize,
    }

    impl BitReader<'_> {
        fn read(&mut self, width: u32) -> u32 {
            let mut value = 0;
            for _ in 0..width {
                let byte = self.bytes[self.bit / 8];
                value = (value << 1) | u32::from((byte >> (7 - self.bit % 8)) & 1);
                self.bit += 1;
            }
            value
        }

        fn next_byte(&mut self) {
            self.bit = self.bit.div_ceil(8) * 8;
        }
    }

    fn two_pages() -> Hints {
        Hints {
            first_page_offset:   500,
            pages:               vec![
                PageHint { objects: 4, length: 900, shared: vec![] },
                PageHint { objects: 2, length: 300, shared: vec![1, 3] },
            ],
            shared_lengths:      vec![100, 200, 300, 400],
            first_page_shared:   4,
            first_shared_object: 7,
            first_shared_offset: 2000,
            outline:             None,
        }
    }

    #[test]
    fn widths_cover_the_greatest_difference_and_are_never_zero() {
        assert_eq!(width(0), 1);
        assert_eq!(width(1), 1);
        assert_eq!(width(2), 2);
        assert_eq!(width(600), 10);
        assert_eq!(width(u32::MAX), 32);
    }

    #[test]
    fn the_page_offset_table_reads_back_item_by_item() {
        let stream = two_pages().encode();
        let mut reader = BitReader { bytes: &stream.data, bit: 0 };
        let header: Vec<u32> = [32, 32, 16, 32, 16, 32, 16, 32, 16, 16, 16, 16, 16].iter().map(|&w| reader.read(w)).collect();
        // least objects 2, page 1 at 500, objects in 2 bits, least length 300, lengths in 10 bits, no content offsets,
        // content as long as the page, counts of shared objects in 2 bits, their ids (up to 4) in 3 bits.
        assert_eq!(header, [2, 500, 2, 300, 10, 0, 0, 300, 10, 2, 3, 0, 1]);

        assert_eq!([reader.read(2), reader.read(2)], [2, 0]);
        reader.next_byte();
        assert_eq!([reader.read(10), reader.read(10)], [600, 0]);
        reader.next_byte();
        assert_eq!([reader.read(2), reader.read(2)], [0, 2]);
        reader.next_byte();
        assert_eq!([reader.read(3), reader.read(3)], [1, 3]);
        reader.next_byte();
        assert_eq!([reader.read(10), reader.read(10)], [600, 0]);
        reader.next_byte();
        assert_eq!(reader.bit / 8, stream.shared, "the shared object table follows");
    }

    #[test]
    fn the_shared_object_table_starts_where_s_says() {
        let stream = two_pages().encode();
        let mut reader = BitReader { bytes: &stream.data[stream.shared..], bit: 0 };
        let header: Vec<u32> = [32, 32, 32, 32, 16, 32, 16].iter().map(|&w| reader.read(w)).collect();
        assert_eq!(header, [7, 2000, 4, 4, 0, 100, 9]);
        let lengths: Vec<u32> = (0..4).map(|_| reader.read(9)).collect();
        assert_eq!(lengths, [0, 100, 200, 300]);
        reader.next_byte();
        let signatures: Vec<u32> = (0..4).map(|_| reader.read(1)).collect();
        assert_eq!(signatures, [0, 0, 0, 0]);
        reader.next_byte();
        assert_eq!(reader.bit / 8 + stream.shared, stream.data.len());
        assert_eq!(stream.outline, None);
    }

    #[test]
    fn the_outline_table_comes_last() {
        let outline = OutlineHint { first_object: 30, offset: 4000, objects: 5, length: 250 };
        let stream = Hints { outline: Some(outline), ..two_pages() }.encode();
        let at = stream.outline.unwrap();
        let mut reader = BitReader { bytes: &stream.data[at..], bit: 0 };
        let table: Vec<u32> = (0..4).map(|_| reader.read(32)).collect();
        assert_eq!(table, [30, 4000, 5, 250]);
        assert_eq!(at + 16, stream.data.len());
    }
}
