# Pagination

`page` is one-based. `page` and `cursor` are mutually exclusive. Signed cursor interfaces are implemented for future opaque provider continuation, but the initial public engines use numeric pages only.

- arXiv: ten upstream results, `start = (page - 1) * 10`.
- Wikipedia: ten results, `offset = (page - 1) * 10`.
- DuckDuckGo HTML: page one only. Later pages require query-bound VQD state and are explicitly unsupported in this cycle.
