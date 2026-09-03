#!/usr/bin/env python3
"""Deterministic, sentence-grounded reconciliation for Text2KGBench triples."""

from __future__ import annotations

import re


ALLOWED_RELATIONS = {
    "director", "screenwriter", "genre", "based on", "cast member",
    "award received", "production company", "country of origin",
    "publication date", "characters", "narrative location",
    "filming location", "main subject", "nominated for", "cost",
}
PLACEHOLDERS = {
    "film", "human", "city", "country", "genre", "award", "amount",
    "written work", "film character", "film production company",
    "film organization", "writing", "logic",
}


def compact(value: str) -> str:
    return re.sub(r"[^a-z0-9]+", "", value.lower())


def canonical_relation(value: str) -> str:
    return re.sub(r"[_\s]+", " ", value).strip().lower()


def mentioned(value: str, sentence: str) -> bool:
    needle = compact(value)
    return bool(needle) and needle in compact(sentence)


def _clean_title(value: str) -> str:
    value = value.strip(' \t\n\r"“”)')
    value = re.sub(r"^The feature film directorial debut of [^,]+,\s*", "", value)
    value = re.sub(r"\s*\([^)]*$", "", value).strip()
    return value


def subjects(sentence: str, triples: list[list[str]]) -> list[str]:
    found: list[str] = []
    for triple in triples:
        if len(triple) == 3 and mentioned(str(triple[0]), sentence):
            found.append(str(triple[0]).strip())

    match = re.match(r'\s*["“]?(.+?)(?:\s*\([^)]*\))?\s+(?:is|was)\b', sentence)
    if match:
        found.append(_clean_title(match.group(1)))

    match = re.search(r",\s*([^,]+?)\s+was the first\b", sentence)
    if match:
        found.append(_clean_title(match.group(1)))

    match = re.search(r"\bin (?P<title>[A-Z][^.;]+?)'s feature film debut\b", sentence)
    if match:
        found.append(_clean_title(match.group("title")))

    # Ordered de-duplication. Generic anaphora is deliberately not a subject.
    result = []
    for value in found:
        if compact(value) not in {"thefilm", "theseries", "film", "series"} and value not in result:
            result.append(value)
    return result


def _people(clause: str) -> list[str]:
    clause = re.split(r",|\b(?:animated|directed|produced|written|released|with|in)\b", clause)[0]
    values = re.split(r"\s+and\s+", clause)
    return [value.strip(" .") for value in values if re.fullmatch(r"[A-Z][\w.'’-]+(?:\s+[A-Z][\w.'’-]+)+", value.strip(" ."))]


def _add_for_subjects(output: set[tuple[str, str, str]], titles: list[str], relation: str,
                      objects: list[str]) -> None:
    for title in titles:
        for obj in objects:
            output.add((title, relation, obj))


def recover(sentence: str, titles: list[str]) -> set[tuple[str, str, str]]:
    output: set[tuple[str, str, str]] = set()
    if not titles:
        return output

    # Role lists sharing one trailing "by" ("written and directed by", or
    # "written, directed, produced ... by") are common in film prose.
    for match in re.finditer(
        r"(?P<roles>(?:written|directed|produced)[^.;]{0,100}?)\s+by\s+"
        r"(?P<people>[A-Z][^.;]+?)(?=,|\.|\s+in\s+|\s+for\s+|\s+and\s+(?:written|produced|released)\s+by)",
        sentence,
    ):
        people = _people(match.group("people"))
        roles = set(re.findall(r"written|directed|produced", match.group("roles")))
        if "directed" in roles:
            _add_for_subjects(output, titles, "director", people)
        if "written" in roles:
            _add_for_subjects(output, titles, "screenwriter", people)

    for relation, marker in (("director", "directed"), ("screenwriter", "written")):
        for match in re.finditer(
            rf"\b{marker}\s+by\s+(?P<people>[A-Z][^.;]+?)(?=,|\.|\s+and\s+(?:written|produced|released)\s+by)",
            sentence,
        ):
            _add_for_subjects(output, titles, relation, _people(match.group("people")))

    for marker in ("produced by", "released by"):
        for match in re.finditer(
            rf"\b{marker}\s+(?P<companies>[A-Z][^.;]+?)(?=,|\.|\s+and\s+(?:written|directed)\s+by)",
            sentence,
        ):
            _add_for_subjects(output, titles, "production company", _people(match.group("companies")))

    # Fronted credit clauses put the title after the people list.
    for title in titles:
        match = re.match(rf"Directed by (?P<people>.+?)\s+{re.escape(title)}\s+stars\b", sentence)
        if match:
            _add_for_subjects(output, [title], "director", _people(match.group("people")))

    animated_by = re.search(r"\banimated by (?P<company>[A-Z][\w .'-]+?)(?=\s+for\s+|,|\.)", sentence)
    if animated_by:
        _add_for_subjects(output, titles, "production company",
                          [animated_by.group("company").strip()])

    for phrase, value in (
        (r"\bfantasy film\b", "Fantasy film"),
        (r"\bsilent film\b", "Silent film"),
        (r"\banime film\b", "Anime"),
    ):
        if re.search(phrase, sentence, re.IGNORECASE):
            _add_for_subjects(output, titles, "genre", [value])

    debut = re.search(r"directorial debut of (?P<person>[A-Z][^,]+),\s*(?P<title>[^,]+?) was", sentence)
    if debut:
        output.add((debut.group("title").strip(), "director", debut.group("person").strip()))

    company = re.search(r"feature film from (?P<company>[A-Z][\w .'-]+)", sentence)
    if company:
        _add_for_subjects(output, titles, "production company", [company.group("company").strip(" .")])

    return output


def reconcile(sentence: str, triples: list[list[str]]) -> list[list[str]]:
    """Keep grounded ontology triples and add conservative pattern recoveries."""
    titles = subjects(sentence, triples)
    reconciled: set[tuple[str, str, str]] = set()
    for triple in triples:
        if not isinstance(triple, list) or len(triple) != 3:
            continue
        subject, relation, obj = map(str, triple)
        relation = canonical_relation(relation)
        if (relation in ALLOWED_RELATIONS and compact(obj) not in {compact(v) for v in PLACEHOLDERS}
                and mentioned(subject, sentence) and mentioned(obj, sentence)):
            reconciled.add((subject.strip(), relation, obj.strip()))
    reconciled |= recover(sentence, titles)
    return [list(triple) for triple in sorted(reconciled)]
