# Open with... does not open the file if Obsidian is not running

*Bug reports · tracked · mikar · 5 October 2026*

**Once you’ve done the above, delete everything above this line.**

### Steps to reproduce

1. Quit Obsidian if it is running
2. From the Finder, “Open with… → Obsidian” a Markdown file
3. … or alternatively, run `open -a Obsidian <file>.md` from a shell

### Did you read and follow the [Troubleshooting Guide](https://forum.obsidian.md/t/about-the-bug-reports-category/24/11) ? \[Yes/No\]

Yes

### Expected result

The file should open in Obsidian, in a new tab.

### Actual result

The file opens **only** if Obsidian had an empty new tab open and focused when it was quit. If not, Obsidian will display a popup saying “Opening FILE”, but not actually open the file (the popup does not seem to display every time, though).

This applies equally to files inside existing vaults, and files outside of vaults.

### Environment

SYSTEM INFO:\
 Obsidian version: 1.14.4\
 Installer version: 1.14.4\
 Operating system: Darwin Kernel Version 25.6.0: Tue Aug 18 17:49:13 PDT 2026; root:xnu-12377.161.15.700.19~2/RELEASE\_ARM64\_T6000 25.6.0\
 Login status: not logged in\
 Language: en\
 Insider build toggle: off\
 Live preview: off\
 Base theme: dark\
 Community theme: AnuPpuccin 1.5.0\
 Snippets enabled: 8\
 Restricted mode: off\
 Plugins installed: 28\
 Plugins enabled: 20\
 1: Style Settings v1.0.9\
 2: Excalidraw v2.28.1\
 3: Advanced Tables v0.23.2\
 4: Calendar v1.5.10\
 5: Emoji Shortcodes v2.2.0\
 6: Kanban v2.0.51\
 7: Tag Wrangler v0.6.5\
 8: Local Images Plus v0.16.4\
 9: File Info Panel v2.0.2\
 10: Focus Mode v1.12.0\
 11: Local Img Plugin v1.0.1\
 12: Dataview v0.5.68\
 13: Numerals v1.10.2\
 14: Git v2.41.1\
 15: Pandoc Plugin v0.4.1\
 16: Vimrc Support v0.10.2\
 17: Vim Toggle v1.1.0\
 18: Sheets Extended v2.0.3\
 19: Code Styler v1.1.7\
 20: Minimal Theme Settings v9.0.0

### Additional information

Note that if Obsidian **is** running when a file is opened from the Finder, the file opens, but replaces the file that was open in the focused tab - this has already been reported in a separate issue.

###

## 5 replies

**WhiteNoise** · [5 October 2026](https://forum.obsidian.md/t/open-with-does-not-open-the-file-if-obsidian-is-not-running/119080/2)

I can’t reproduce this. It works fine for me.

Enable restricted mode. Restart obsidian and try again.

**mikar** · [6 October 2026](https://forum.obsidian.md/t/open-with-does-not-open-the-file-if-obsidian-is-not-running/119080/3)

Enabled restricted mode, and still happens.

Made screen recordings, but can’t figure out how to attach them here, so uploaded:

![](https://www.swisstransfer.com/favicon.ico) [SwissTransfer](https://www.swisstransfer.com/dl/01a11331-840c-7313-b94a-03f9e7f62e4a)

![](https://www.swisstransfer.com/og.webp)

### [SwissTransfer - Send large files securely and free of charge](https://www.swisstransfer.com/dl/01a11331-840c-7313-b94a-03f9e7f62e4a)

Send up to 50 GB - Free and no registration required - Keep your transfers for 30 days

`with_file_open.mov` illustrates the case where a file is already open in Obsidian (new file not loaded if Obsidian is not running). “Opening with…” or `"open -a Obsidian"` again once Obsidian is running then loads the file (but replaces the tab content instead of opening a new one as it should).

`with_empty_tab_open.mov` shows the file loading successfully if Obsidian was showing an empty tab when it was closed.

**WhiteNoise** · [7 October 2026](https://forum.obsidian.md/t/open-with-does-not-open-the-file-if-obsidian-is-not-running/119080/4)

The remplacement of the current note is how it works currently. Perhaps we will change it later.

1. CLose everything
2. Create a new empty vault and leave one note open.
3. Close the vault
4. Make a screen recording of the bug happening with “Open with” menu in finder.

Thanks

**mikar** · [13 October 2026](https://forum.obsidian.md/t/open-with-does-not-open-the-file-if-obsidian-is-not-running/119080/5)

Tested with a new vault. Observations:

- If only the new empty vault is open, the issue does NOT occur
- If both the new vault and my main vault are open, the issue occurs as before, regardless of which vault window was focused before quitting.
- Additionally, having an empty tab open in my main vault allows the Opened with document to open in that tab. If the empty tab is open in the new vault, the document does not open.

In all tests, Finder “Open With…” and `open -a obsidian` have behaved identically.

Understand replacing the current note is intended behavior currently, yet still argue that’s not the right choice: it makes sense when opening a document from inside Obsidian, but with “Open with…”, especially if Obsidian was closed, this causes you to silently lose context - a document you might have been working on is replaced with another one, without you noticing. But appreciate this is a separate issue.

**WhiteNoise** · [13 October 2026](https://forum.obsidian.md/t/open-with-does-not-open-the-file-if-obsidian-is-not-running/119080/6)

Please, let’s try to remain on topic.

The topic is “\[Open with… does not open the file if Obsidian is not running\]”

Please, show me this happening when only a new empty vault is involved.
