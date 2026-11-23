# Syntax sample: Markdown

Setext heading
==============

Smaller setext heading
----------------------

## Emphasis

Some *emphasis*, some _underscore emphasis_, some **strong text**, some __strong underscores__,
***both at once***, ~~strikethrough~~ and a `inline code` span with ``double `ticks` inside``.
A hard line break ends with two spaces  
and a backslash\
and an escaped \*star\* stays plain.

### Links and images

An [inline link](https://example.com "with a title"), a [reference link][ref], a [collapsed][] one,
an autolink <https://example.com/path?query=1#hash>, a bare https://example.com url,
an email <someone@example.com>, and an image ![alt text](https://example.com/logo.png "Logo").

[ref]: https://example.com/reference "Reference title"
[collapsed]: <https://example.com/collapsed>

#### Lists

- First item
- Second item
  - Nested item
    - Deeper item
* Star marker
+ Plus marker

1. First step
2. Second step
   1. Nested step
10) Parenthesis marker

- [x] Done task
- [ ] Open task

##### Quotes

> A quote with **strong** text and `code`.
>
> > A nested quote.

###### Code

Indented code block:

    let indented = true;

```rust
fn main() {
  println!("fenced with a language");
}
```

```
fenced without a language
```

~~~ts
const tilde: number = 1;
~~~

## Tables

| Name  | Value | Notes          |
| ----- | ----: | :------------- |
| alpha |     1 | `code` in cell |
| beta  |     2 | **strong**     |

---

***

<details>
<summary>Inline HTML</summary>

<span style="color: red">raw html</span> and an entity: &copy; &amp; &#169;

</details>

<!-- An html comment. -->

A footnote reference[^1] and a definition below.

[^1]: The footnote text.
