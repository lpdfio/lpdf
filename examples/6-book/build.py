#!/usr/bin/env python3
"""Builds document.xml, a pocket-size book, from source/alice.txt.

    python build.py [--starts 3,12,21,...]

document.xml is a build output: change this script or the source text, not the XML.

The text is Lewis Carroll's Alice's Adventures in Wonderland (1865), all twelve chapters, which is in the
public domain. source/ holds it with the distributor's header and footer removed. The plates are by Arthur
Rackham (1907), in assets/images.

--starts is the page each chapter begins on, twelve numbers. Lpdf cannot tell the contents page or the running
headers where a chapter landed, so the numbers are measured and passed in: build, render, read where the
chapters begin (each begins a new page), build again with those numbers. Without it every chapter is guessed
to be eight pages long, which is wrong, so the contents and the headers are wrong until it is measured.
"""
import math
import pathlib
import re
import sys

HERE = pathlib.Path(__file__).parent
SOURCE = HERE / 'source' / 'alice.txt'
OUT = HERE / 'document.xml'

# the plate that opens each chapter: file, and its size in pixels, for the shape of the picture
IMAGES = {
    'I':    ('chapter-01-alice.jpg',              256, 500),
    'II':   ('chapter-02-pool-of-tears.jpg',      373, 500),
    'III':  ('chapter-03-caucus-race.jpg',        359, 500),
    'IV':   ('chapter-04-white-rabbit.jpg',       353, 500),
    'V':    ('chapter-05-caterpillar.jpg',        337, 500),
    'VI':   ('chapter-06-pig-and-pepper.jpg',     384, 500),
    'VII':  ('chapter-07-tea-party.jpg',          359, 500),
    'VIII': ('chapter-08-croquet-ground.jpg',     364, 500),
    'IX':   ('chapter-09-mock-turtle-story.jpg',  364, 500),
    'X':    ('chapter-10-lobster-quadrille.jpg',  372, 500),
    'XI':   ('chapter-11-trial.jpg',              359, 500),
    'XII':  ('chapter-12-evidence.jpg',           356, 500),
}
PLATE_HEIGHT = 164   # points; centred, across and down, in a frame that ends where the title begins
FRAME_HEIGHT = 258   # points: 56 of margin + 258 + 12.5 of gap and padding puts the title at 66% of the 495pt page

starts = [3 + 8 * i for i in range(12)]
if '--starts' in sys.argv:
    starts = [int(n) for n in sys.argv[sys.argv.index('--starts') + 1].split(',')]


def esc(s):
    return s.replace('&', '&amp;').replace('<', '&lt;').replace('>', '&gt;')


def inline(s):
    """One paragraph's lines joined, escaped, with _emphasis_ turned into an italic span."""
    s = esc(' '.join(line.strip() for line in s.split('\n')))
    # emphasis is left plain: see the comment in the file this writes, on spans
    return s.replace('_', '')


def roman_title(number):
    return 'CHAPTER ' + number


# ---- read the chapters -------------------------------------------------------------------------------------------
chapters = []
for part in re.split(r'\n(?=CHAPTER [IVXL]+\.\n)', SOURCE.read_text(encoding='utf-8').strip() + '\n'):
    lines = part.strip().split('\n')
    number = lines[0].strip().rstrip('.').split()[1]
    title = lines[1].strip()
    blocks = [b for b in re.split(r'\n\s*\n', '\n'.join(lines[2:]).strip()) if b.strip()]
    chapters.append({'number': number, 'title': title, 'blocks': blocks})


def block_xml(block, pad):
    """One block of the source as Lpdf elements: a paragraph, an indented passage, a stanza, or a break."""
    lines = block.split('\n')
    if re.fullmatch(r'[\s*]+', block):
        return 'BREAK'
    if block.strip() == 'THE END':
        return '%s<stack padding="22pt 0pt 0pt 0pt"><text font="heading" color="ink" align="center">THE END</text></stack>' % pad
    indents = [len(l) - len(l.lstrip(' ')) for l in lines]
    if min(indents) >= 5:
        return ('%s<stack padding="0pt 0pt 0pt 20pt">\n%s  <text color="ink">%s</text>\n%s</stack>'
                % (pad, pad, inline(block), pad))
    # verse: the lines are short. A wrapped paragraph fills its first line to about 70 characters, so a short
    # first line is verse, and so are three lines or more that are all under 63 (a two-line paragraph can be)
    widest = max(len(l.strip()) for l in lines)
    if len(lines) >= 2 and (len(lines[0].strip()) <= 52 or (len(lines) >= 3 and widest <= 62)):
        rows = []
        for line, indent in zip(lines, indents):
            text = '<text color="ink">%s</text>' % inline(line)
            if indent:
                rows.append('%s  <stack padding="0pt 0pt 0pt %dpt">%s</stack>' % (pad, indent * 3, text))
            else:
                rows.append('%s  %s' % (pad, text))
        return '%s<stack gap="0pt" padding="0pt 0pt 0pt 14pt" paginate="no">\n%s\n%s</stack>' % (pad, '\n'.join(rows), pad)
    return '%s<text color="ink">%s</text>' % (pad, inline(block))


def chapter_xml(ch):
    pad = '        '
    out = [
        '%s<stack gap="s" font-size="m" paginate="break-before">' % pad,
        '%s  <frame height="%dpt"><img name="chapter-%s" width="%dpt" height="%dpt" /></frame>'
        % (pad, FRAME_HEIGHT, ch['number'].lower(), round(PLATE_HEIGHT * IMAGES[ch['number']][1] / IMAGES[ch['number']][2]), PLATE_HEIGHT),
        '%s  <stack gap="s" padding="8pt 0pt 18pt 0pt">' % pad,
        '%s    <text font-size="xs" color="muted">%s</text>' % (pad, roman_title(ch['number'])),
        '%s    <text font="heading" font-size="xl" color="ink">%s</text>' % (pad, esc(ch['title'])),
        '%s    <stack width="26pt" height="1.5pt" background="gold-dark" />' % pad,
        '%s  </stack>' % pad,
    ]
    previous_break = False
    blocks = [b for b in ch['blocks'] if not b.lstrip().startswith('[later editions')]   # verses that only later
    for n, block in enumerate(blocks):                                                    # editions added
        if block.strip() == 'THE END':
            continue   # set below, with the paragraph before it
        xml = block_xml(block, pad + '  ')
        if n + 1 < len(blocks) and blocks[n + 1].strip() == 'THE END':
            # the last paragraph and THE END in one stack that is never split, so that the end does not
            # land alone on a page of its own (keep-next does not work between children of a stack)
            xml = '%s  <stack gap="s" paginate="no">\n  %s\n  %s\n%s  </stack>' % (
                pad, xml, block_xml(blocks[n + 1], pad + '  '), pad)
        if xml == 'BREAK':
            if not previous_break:
                out.append('%s  <text color="muted" align="center">*     *     *</text>' % pad)
            previous_break = True
            continue
        previous_break = False
        out.append(xml)
    out.append('%s</stack>' % pad)
    return '\n'.join(out)


# ---- the cover's watch, drawn on the canvas ----------------------------------------------------------------------
CX, CY = 153, 322


def polar(radius, degrees):
    a = math.radians(degrees)
    return CX + radius * math.sin(a), CY - radius * math.cos(a)


watch = ['<circle cx="%dpt" cy="%dpt" r="58pt" stroke="gold" stroke-width="2.5pt" />' % (CX, CY),
         '<circle cx="%dpt" cy="%dpt" r="51pt" stroke="gold" stroke-width="0.6pt" />' % (CX, CY),
         '<rect x="147pt" y="%dpt" w="12pt" h="9pt" radius="2pt" fill="gold" />' % (CY - 67),
         '<circle cx="%dpt" cy="%dpt" r="6pt" stroke="gold" stroke-width="1.6pt" />' % (CX, CY - 76)]
for hour in range(12):
    inner = 41 if hour % 3 == 0 else 44
    x1, y1 = polar(inner, hour * 30)
    x2, y2 = polar(49, hour * 30)
    watch.append('<line x1="%.1fpt" y1="%.1fpt" x2="%.1fpt" y2="%.1fpt" stroke="gold" stroke-width="%s" />'
                 % (x1, y1, x2, y2, '1.4pt' if hour % 3 == 0 else '0.8pt'))
hx, hy = polar(27, 305)   # ten past ten: the hour hand
mx, my = polar(38, 60)    # and the minute hand
watch.append('<line x1="%dpt" y1="%dpt" x2="%.1fpt" y2="%.1fpt" stroke="gold" stroke-width="2.2pt" line-cap="round" />' % (CX, CY, hx, hy))
watch.append('<line x1="%dpt" y1="%dpt" x2="%.1fpt" y2="%.1fpt" stroke="gold" stroke-width="1.4pt" line-cap="round" />' % (CX, CY, mx, my))
watch.append('<circle cx="%dpt" cy="%dpt" r="3pt" fill="gold" />' % (CX, CY))

# ---- headers: a canvas layer for each chapter, on the pages of that chapter after its first ----------------------
# Drawn on the canvas, not as regions: Lpdf reserves room for every region whose page range it cannot count,
# on every page, so twelve of them left no room for text. A canvas layer takes a page range and reserves nothing.
headers = []
for i, ch in enumerate(chapters):
    first = starts[i] + 1
    last = (starts[i + 1] - 1) if i + 1 < len(chapters) else 'last'
    if last != 'last' and first > last:
        continue
    headers.append('''        <layer page="%s-%s">
          <text x="34pt" y="26pt" w="238pt" font-size="7pt" color="muted" align="center">%s  ·  %s</text>
          <line x1="34pt" y1="38pt" x2="272pt" y2="38pt" stroke="rule" stroke-width="0.5pt" />
        </layer>''' % (first, last, roman_title(ch['number']), esc(ch['title'].upper())))

contents = []
for i, ch in enumerate(chapters):
    contents.append('''            <tr>
              <td padding="4pt 0pt"><text font-size="s" color="muted">%s</text></td>
              <td padding="4pt 0pt"><text font-size="m" color="ink">%s</text></td>
              <td padding="4pt 0pt"><text font-size="m" color="ink" align="right">%d</text></td>
            </tr>''' % (ch['number'], esc(ch['title']), starts[i]))

images_xml = '\n'.join('    <image name="chapter-%s" src="assets/images/%s" />' % (n.lower(), v[0])
                       for n, v in IMAGES.items())

xml = '''<lpdf version="1">
  <!--
    A pocket book, all twelve chapters of Alice's Adventures in Wonderland, 4.25 by 6.875 inches. It is
    generated: build.py turns source/alice.txt into this file, so change the script or the text
    and not the XML. The text is public domain (Lewis Carroll, 1865). What this adds to the brochure:

      size           two lengths, in points, for a size with no name: 306pt 495pt is a mass-market paperback
      one section    the whole book is one, so the page numbers run on from the cover to the last page
      paginate       paginate="break-before" on a direct child of the layout starts a new page. Each
                     chapter, and the contents, is such a child, so each begins at the top of a page. On
                     a box inside another box it is ignored, and paginate="no" keeps a stanza whole
      headers        the running head, the chapter's number and name, is a canvas layer with a page range:
                     page="4-9" shows it on those pages and no others. There is one for each chapter, and
                     none on a chapter's first page, where the plate is. They are on the canvas and not
                     regions because Lpdf reserves room for every region whose pages it cannot count, on
                     every page: twelve regions left no room for text. A layer reserves nothing, so the
                     page's top margin, 56pt, holds the head, with 18pt between its rule and the text
      footer         a region, the last child of the layout: {page} on every page after the cover
      type           Crimson Text, a book face, in regular, italic and semibold, embedded from assets/fonts
                     (SIL Open Font License). The italic is used on the cover only: see the last paragraph
      canvas         the cover is drawn: a panel, a frame and a pocket watch, from rects, circles and lines
      plates         each chapter opens with a picture in the top two-thirds of the page: an img inside a
                     frame 258pt tall, which centres it across and down, and the chapter's title begins
                     below it, at 66%% of the page height, with its text after it. The plates are by Arthur Rackham (1907, public
                     domain); their sizes are set in points, in the proportions of the originals

    Where the chapters begin is not something Lpdf can look up, so the page numbers in the contents and the
    ranges of the headers are measured and passed to the script, which last built this with the starts
    %s. After a change to the text, build, render, read where the chapters begin, and build again.

    The words the author emphasised are set plain, not in italic. A span is placed from the width of the
    text before it, and for an embedded font Lpdf measures every character outside plain ASCII, such as the
    curly quotes and the dash, at one default width. A line with a span after any of them is drawn with the
    span too early or too late, and in a book nearly every line has one. Straight quotes would hide it, and a
    book with straight quotes is worse.

    The lines are not justified: in this version of Lpdf justification is not applied to an embedded font
    (a core font is justified, but spans inside it are drawn wrongly). Line height is fixed at 1.2
    times the size, so the paragraphs are separated by a gap and not by a first-line indent.
  -->
  <assets>
    <font name="body"    src="assets/fonts/CrimsonText-Regular.ttf" />
    <font name="italic"  src="assets/fonts/CrimsonText-Italic.ttf" />
    <font name="heading" src="assets/fonts/CrimsonText-SemiBold.ttf" />
@@IMAGES@@
  </assets>

  <tokens>
    <colors>
      <color name="paper"     value="#faf6ec" />
      <color name="ink"       value="#26221c" />
      <color name="muted"     value="#6b6458" />
      <color name="rule"      value="#d9d2c0" />
      <color name="gold"      value="#d6b25e" />
      <color name="gold-dark" value="#9a7a2c" />
      <color name="cover"     value="#1c3550" />
      <color name="cream"     value="#f3ead6" />
    </colors>
    <space xs="2pt" s="4.5pt" m="9pt" l="14pt" xl="22pt" xxl="34pt" />
    <text-size xs="7pt" s="8pt" m="10pt" l="12pt" xl="18pt" xxl="30pt" />
  </tokens>

  <document size="306pt 495pt" margin="56pt 34pt 44pt 34pt" font="body">
    <meta title="Alice’s Adventures in Wonderland" subject="Pocket book" author="Lewis Carroll" />
    <section background="paper">

      <!-- the cover, on the first page only -->
      <canvas>
        <layer page="first">
          <rect x="0pt" y="0pt" w="306pt" h="495pt" fill="cover" />
          <rect x="14pt" y="14pt" w="278pt" h="467pt" stroke="gold" stroke-width="0.75pt" />
          %s
        </layer>

        <!-- the running heads, one layer for each chapter -->
%s
      </canvas>

      <layout>

        <!-- the cover: the author at the top, the title, a line at the foot, with the watch between -->
        <stack height="380pt" justify="between" padding="8pt 0pt 0pt 0pt">
          <stack gap="xl">
            <text font-size="s" color="gold" align="center">LEWIS CARROLL</text>
            <stack gap="xs">
              <text font="heading" font-size="xxl" color="cream" align="center">Alice’s</text>
              <text font="heading" font-size="xxl" color="cream" align="center">Adventures</text>
              <text font="heading" font-size="xxl" color="cream" align="center">in Wonderland</text>
            </stack>
          </stack>
          <text font="italic" font-size="m" color="gold" align="center">With plates by Arthur Rackham</text>
        </stack>

        <!-- the contents, on a page of its own -->
        <stack gap="m" font-size="m" paginate="break-before" padding="26pt 0pt 0pt 0pt">
          <stack gap="s">
            <text font="heading" font-size="xl" color="ink">Contents</text>
            <stack width="26pt" height="1.5pt" background="gold-dark" />
          </stack>
          <table cols="2fr 11fr 2fr">
%s
          </table>
          <text font="italic" font-size="s" color="muted">Illustrations by Arthur Rackham, 1907.</text>
        </stack>

%s

        <!-- the footer: the last child of the layout, the page number on every page after the cover -->
        <region pin="bottom" page="2-last">
          <text font-size="s" color="muted" align="center">{page}</text>
        </region>

      </layout>
    </section>
  </document>
</lpdf>
''' % (','.join(str(n) for n in starts), '\n          '.join(watch), '\n'.join(headers),
       '\n'.join(contents), '\n\n'.join(chapter_xml(ch) for ch in chapters))
xml = xml.replace('@@IMAGES@@', images_xml)

with open(OUT, 'w', encoding='utf-8', newline='\n') as f:
    f.write(xml)
print('wrote %s (%d lines), chapters begin on pages %s' % (OUT.name, xml.count('\n'), starts))
