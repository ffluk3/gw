# Writing the docs

These rules exist because the docs once read as stiff and mechanism-first. Each
one fixes a problem that actually showed up.

## Audience

1. **Know which manual you are writing.** If a reader only runs `gw`, the page
   belongs in the User Manual. If a reader only changes `gw`, it belongs in the
   Developer Manual. A page that serves both is doing two jobs; split it.

2. **Do not explain the build to a user, or the commands to a contributor.**
   Move the section rather than adding a caveat.

## Openings

3. **Narrative pages open with the problem, not the mechanism.** The home page
   must state the annoyance and the fix before it names the directory layout.
   A reader should be able to say what `gw` is for without scrolling.

4. **Show a command before you explain why it works.** A before/after block
   lands faster than a paragraph of motivation.

## Words

5. **Define a term at its first use, in the manual where it first appears.**
   Gloss `bare repository`, `worktree`, `layout`, `repos root`, `provider`, and
   `convention` the first time a user meets them. Gloss `verdict`,
   `observation`, and `identity strength` in the Developer Manual, not before.

6. **One word, one meaning.** "Layout" always means the bare repository plus its
   worktrees. Do not also use it for "directory".

7. **Plain English for narrative; Simplified Technical English for procedures.**
   Home, User Manual, and Developer Manual may use second person, contractions,
   and longer sentences when the longer sentence reads better. How-to steps,
   reference tables, and the ordered safety checks stay short, active, and
   imperative. The safety rules in particular must stay precise — do not soften
   "unknown counts as dirty".

## Structure

8. **Link, do not duplicate.** Before you copy a list, check whether another
   page owns it. The four-bullet "what `gw` adds" list and the bare/worktrees
   tree used to appear in several files; each fact should have one home.

9. **Every page needs a "Next" that is not the section index.** Tell the reader
   what to read after this page.

10. **Keep anchors stable.** `conventions.md` is linked at
    `#why-the-default-is-one-branch-to-one-ticket` and `#evidence-strength`;
    `reference/cli.md` at `#shell-integration` and
    `reference/providers.md` at `#kind-command`. Moving a page is fine if you
    update the links; renaming a heading silently breaks them.

11. **Do not rename `docs/explanation/`.** Its nav label is "User Manual", but
    `skills/gw/SKILL.md` links the published pages by absolute GitHub URL. A
    directory rename breaks the shipped skill. Change the label, not the path.

## Examples

12. **Verify example output against the real program.** `gw status` columns are
    `BRANCH / LAST COMMIT / DIRTY / STATE / SUMMARY`; `gw clean` renders
    `BRANCH / ACTION / PATH / STATUS` and then
    `Dry run — nothing changed. Re-run with --yes to apply.` If you add an
    example, run the command or read the renderer before you paste it.

13. **Claims about git must be checked against git.** Do not assert that plain
    `git worktree` loses data in a case where it refuses: `git worktree remove`
    already declines a dirty tree. Prefer the narrower, defensible claim.

## Links and build

14. **Never link from `docs/` to a file outside it.** `AGENTS.md`,
    `CONTRIBUTING.md`, and `skills/` are not in the MkDocs site, and a relative
    link to them fails the build. Name them in backticks, or use an absolute
    `https://github.com/ffluk3/gw/blob/main/...` URL.

15. **Every page must appear in `nav:`.** `mkdocs.yml` sets `strict: true`, so a
    page missing from `nav:` or a broken link is a build error, not a warning.
    Check locally before you open a pull request:

    ```bash
    mkdocs build --strict
    ```
