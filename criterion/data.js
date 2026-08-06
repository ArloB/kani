window.BENCHMARK_DATA = {
  "lastUpdate": 1786026587069,
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
      }
    ]
  }
}