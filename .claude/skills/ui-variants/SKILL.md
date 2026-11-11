---
name: ui-variants
description: Build a comparison sheet of several versions of a UI element so the user can pick by code (like "A3 B2 C1 D4"), then apply the picks. Use when the user is unsure about how something looks, says a UI feels off without being able to say why, asks for options, alternatives or versions of a UI element, or when a visual choice has no clear right answer.
---

# UI variants

Turn a vague visual question into a sheet of real options the user can choose from by code. Seeing real options is faster and more reliable than describing them.

## 1. Diagnose first

If the user can't say what's wrong, look at the element and name the likely causes, with a reason for each. Compare with apps they would know, and say when that comparison comes from memory.

Sort your fixes into two groups:

- **Straightforward:** clearly better, little downside. Offer to apply these without a vote.
- **Matters of taste:** real trade-offs. These go on the sheet.

Apply the straightforward fixes before building the sheet, so the options are judged against the improved version.

## 2. Build the sheet

Create one standalone HTML file next to the design it belongs to. It must load the project's real stylesheet, so the options show the actual tokens and components rather than an imitation.

- **Sections are independent questions:** each section varies exactly one thing, so options from different sections can be combined.
- **Stable codes:** letter for the section, number for the option (`A1`, `A2`, `B1`). Mention that picks can be combined.
- **The first option is the current design**, labelled "(current)". Include a rejected older version if it helps comparison, labelled as such.
- **Each option has** a code, a short name, one plain sentence on the trade-off, and a live render.
- **Use real content** from the user's data, including states that show the differences. For example, selection options need some rows selected and some not.
- **Enough context:** show the element beside its neighbours, since most visual problems are about the relationship between things.
- **Size:** 3 to 5 options per section and 2 to 5 sections. Drop options that differ too little to see.
- **Options are CSS overrides** on a wrapper class where possible. Each should be cheap to apply afterwards.
- **Add a theme toggle** if the product has themes.

Don't change the real design from the sheet, and don't use it for options the user has already rejected, except as the labelled comparison.

## 3. Check it yourself

Screenshot the whole sheet and look at it before reporting. Check that:

- every option actually differs from the current version
- a section's options all render, including any past the bottom of the first screenshot
- text doesn't clip or overlap

## 4. Present it

Give the file's full path and a table with one row per section: what varies, the options, and your pick with a one-line reason. Have an opinion. Ask for picks in code form, like `A3 B2 C1 D4`, and say what happens to any section they skip. Normally it stays as it is.

Say what you did not check, such as a theme you viewed in only one mode.

## 5. Apply the picks

- Apply exactly what was picked, even when it differs from your recommendation.
- Move the chosen CSS into the real stylesheet with proper tokens, not sheet-only classes.
- Check that a picked option still works in the real page. Report anything that looks different from the sheet, such as adjacent rows interacting, and offer a fix instead of quietly changing it.
- Report any deviation you made from the option as shown.
- Record the decision wherever the project keeps design rules.
- Leave the sheet in place as reference, unless the user wants it removed. Its "(current)" labels will go stale, which is fine.

Picks can arrive one at a time. Treat each as an update rather than asking for the full set again.
