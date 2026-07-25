# URL canonicalisation

Canonicalisation uses the `url` crate and:

1. accepts only HTTP and HTTPS;
2. rejects embedded credentials;
3. lowercases the hostname through URL parsing;
4. removes fragments and default ports;
5. normalises empty paths to `/`;
6. removes `utm_*`, `gclid`, `fbclid`, `mc_cid`, `mc_eid` and `ref_src`;
7. sorts retained query pairs;
8. preserves all other content-affecting parameters; and
9. never merges HTTP with HTTPS automatically.

MVP deduplication merges only exact canonical URL matches.
