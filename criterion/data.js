window.BENCHMARK_DATA = {
  "lastUpdate": 1786942454369,
  "repoUrl": "https://github.com/ArloB/kani",
  "entries": {
    "Benchmark": [
      {
        "commit": {
          "author": {
            "name": "Arlo Burke",
            "username": "ArloB",
            "email": "arlo.burke2@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a1448d0fae1fe818161833a52effebb998b59ec0",
          "message": "ci: share one rust cache, drop the fat-LTO bench build, gate bench regressions (#3)\n\n* ci: share one rust cache, drop the fat-LTO bench build, gate bench regressions\n\n* docs: apply COMMENT_STYLE.md to the CI change",
          "timestamp": "2026-08-06T08:02:24Z",
          "url": "https://github.com/ArloB/kani/commit/a1448d0fae1fe818161833a52effebb998b59ec0"
        },
        "date": 1786026586382,
        "tool": "cargo",
        "benches": [
          {
            "name": "blueprint_eval/html_200_rows",
            "value": 3585107,
            "range": "± 9224",
            "unit": "ns/iter"
          },
          {
            "name": "blueprint_eval/json_200_rows",
            "value": 1176676,
            "range": "± 21778",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Arlo Burke",
            "username": "ArloB",
            "email": "arlo.burke2@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d46c8093f1d6415a6485aec566f9d9f93cc4ecae",
          "message": "Feature/stage 4 tiering (#4)\n\n* feat: publish a compatibility tier for every REST operation and CLI command\n\n* feat: promote OPDS and the recovery commands to the stable tier\n\n* fix: rollback verifies a backup, it does not roll one back\n\n* refactor: rename kani-cli rollback to backup-verify\n\n* docs: add the Stage 4 stability statements and report the schema version\n\n* feat: emit API token timestamps as RFC 3339\n\n* chore: remove EXAMPLE_EXTENSION.yaml and correct the built-in sources claim\n\n* chore: stop naming individual extensions across the repo\n\n* fix: resolve client IP through one trusted-proxy aware path\n\n* feat: accept WASM metadata flags when publishing\n\n* feat: expand source compatibility and image transforms\n\n* style: apply repository formatting\n\n* Changed to a single version docs flow",
          "timestamp": "2026-08-07T14:18:44Z",
          "url": "https://github.com/ArloB/kani/commit/d46c8093f1d6415a6485aec566f9d9f93cc4ecae"
        },
        "date": 1786942453384,
        "tool": "cargo",
        "benches": [
          {
            "name": "blueprint_eval/html_200_rows",
            "value": 3621212,
            "range": "± 226139",
            "unit": "ns/iter"
          },
          {
            "name": "blueprint_eval/json_200_rows",
            "value": 1199736,
            "range": "± 10552",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}