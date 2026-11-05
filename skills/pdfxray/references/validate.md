# validate

`pdfxray validate <FILE> [flags]` runs veraPDF on the file (as a separate Java process; Java 11
or later) and maps each failure to a node id.

## Flags

* `--profile <P>`: `ua1` (PDF/UA-1, the default), `ua2` (PDF/UA-2), `wcag22` (WCAG 2.2,
  machine-checkable rules), `wtpdf` (Well-Tagged PDF 1.0, accessibility), `tagged1`,
  `tagged2` (ISO 32000-1 and -2 Tagged PDF), `pdfa1a`, `pdfa1b`, `pdfa2a`, `pdfa2b`, `pdfa2u`,
  `pdfa3a`, `pdfa3b`, `pdfa3u`, `pdfa4`, `pdfa4e`, `pdfa4f`.
* `--profile-file <XML>`: a custom veraPDF profile; overrides `--profile`.
* `--verapdf <PATH>`: the veraPDF launcher or CLI jar to use instead of searching for one.
* `--install`: download and install the pinned veraPDF release when none is found. Without it
  and without veraPDF, the command fails with
  `veraPDF not found; pass --verapdf <path> or --install to download the pinned release`.
* `--timeout <SECONDS>`: give up after this long (default 300).

## Output

```json
{ "profile": "PDF/UA-1 validation profile", "compliant": false, "verapdf_version": "1.30.3",
  "checks_passed": 361, "checks_failed": 3,
  "findings": [
    { "rule": "ISO 14289-1:2014 7.1-10",
      "description": "The document catalog dictionary shall include a ViewerPreferences dictionary containing a DisplayDocTitle key, whose value shall be true",
      "message": "ViewerPreferences dictionary is not present in the document Catalog, or DisplayDocTitle key is set to false or is not present in the ViewerPreferences dictionary (ViewerPreferences = null, DisplayDocTitle = null)",
      "context": "root", "target": "doc", "tags": ["syntax"] } ] }
```

* The command exits 0 whether or not the file complies: read `compliant`.
* `rule` is the clause of the standard; `description` is the rule and `message` what veraPDF
  found. `context` is veraPDF's own path to the failing object.
* `target` is the node id pdfxray mapped the failure to (`doc`, `page:`, `struct:`, `mc:`,
  `res:`…), or `null` when it could not be resolved. Pass it to `node`, or to
  `render --highlight` when it is on a page.
* Each failed check is one finding. Group them by `rule` when you report, since one rule often
  fails on many objects.
