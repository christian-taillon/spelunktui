# Spelunk TUI OpenSpec

## Current Status
- **Search Functionality:**
    - Create search jobs via `POST /services/search/jobs`.
    - Poll job status via `GET /services/search/jobs/{search_id}`.
    - Fetch JSON results via `GET /services/search/jobs/{search_id}/results`.
    - Kill jobs via `DELETE /services/search/jobs/{search_id}`.
- **TUI:**
    - Input screen for SPL queries.
    - Job status display (running/done, event count, duration).
    - Results view (JSON key-value pairs).
    - Shareable URL display.
- **Configuration:** Uses `SPLUNK_BASE_URL` and `SPLUNK_TOKEN`.

## API Documentation Reference
- **Main Splunk REST API Reference:** [https://docs.splunk.com/Documentation/Splunk/latest/RESTREF/RESTprolog](https://docs.splunk.com/Documentation/Splunk/latest/RESTREF/RESTprolog)
- **Search Jobs API:** [https://docs.splunk.com/Documentation/Splunk/latest/RESTREF/RESTsearch#search.2Fjobs](https://docs.splunk.com/Documentation/Splunk/latest/RESTREF/RESTsearch#search.2Fjobs)
- **Search Results API:** [https://docs.splunk.com/Documentation/Splunk/latest/RESTREF/RESTsearch#search.2Fjobs.2F.7Bsearch_id.7D.2Fresults](https://docs.splunk.com/Documentation/Splunk/latest/RESTREF/RESTsearch#search.2Fjobs.2F.7Bsearch_id.7D.2Fresults)

## Proposed Features (ToDo)
- **SPL Syntax Highlighting:** Implement syntax highlighting for the search input to improve usability.
- **Enhanced Result Navigation:** Better pagination and result inspection (e.g., expanding JSON objects).
- **Saved Searches:** Ability to list and run saved searches from Splunk.
- **Export Functionality:** Export results to CSV or JSON file.
- **Interactive Filtering:** Filter results within the TUI without re-running the search.
