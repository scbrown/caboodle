import importlib.util
from pathlib import Path


PATH = Path(__file__).parents[1] / "scripts" / "text2kg_grounded.py"
SPEC = importlib.util.spec_from_file_location("text2kg_grounded", PATH)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def test_filters_ontology_placeholders_and_demo_leakage():
    sentence = "Keyboard Cat was made in 1984 by Charlie Schmidt."
    triples = [
        ["Keyboard Cat", "director", "film"],
        ["Example Film", "director", "Example Person"],
        ["Keyboard Cat", "screenwriter", "Charlie Schmidt"],
    ]
    assert MODULE.reconcile(sentence, triples) == [
        ["Keyboard Cat", "screenwriter", "Charlie Schmidt"]
    ]


def test_recovers_shared_writer_and_director():
    sentence = "Metal Skin Panic MADOX-01 was directed and written by Shinji Aramaki."
    assert MODULE.reconcile(sentence, []) == [
        ["Metal Skin Panic MADOX-01", "director", "Shinji Aramaki"],
        ["Metal Skin Panic MADOX-01", "screenwriter", "Shinji Aramaki"],
    ]


def test_recovers_lists_without_inventing_generic_subject():
    sentence = "The film is directed by Alice Smith and Bob Jones."
    assert MODULE.reconcile(sentence, []) == []


def test_recovers_directorial_debut_and_company():
    sentence = ("The feature film directorial debut of John Lasseter, Toy Story was the first "
                "entirely computer-animated feature film, as well as the first feature film from Pixar.")
    assert MODULE.reconcile(sentence, []) == [
        ["Toy Story", "director", "John Lasseter"],
        ["Toy Story", "production company", "Pixar"],
    ]


def test_coordination_does_not_assign_next_role_to_writer():
    sentence = ("Example Film is produced by Studio One, written by Alice Smith and directed by "
                "Bob Jones and Carol White.")
    assert MODULE.reconcile(sentence, []) == [
        ["Example Film", "director", "Bob Jones"],
        ["Example Film", "director", "Carol White"],
        ["Example Film", "production company", "Studio One"],
        ["Example Film", "screenwriter", "Alice Smith"],
    ]


def test_fronted_directors_and_grounded_genre():
    sentence = ("Directed by Richard Eichberg and Walter Summers The Flame of Love stars "
                "Anna May Wong in a silent film.")
    triples = [["The Flame of Love", "cast_member", "Anna May Wong"]]
    assert MODULE.reconcile(sentence, triples) == [
        ["The Flame of Love", "cast member", "Anna May Wong"],
        ["The Flame of Love", "director", "Richard Eichberg"],
        ["The Flame of Love", "director", "Walter Summers"],
        ["The Flame of Love", "genre", "Silent film"],
    ]


def test_fans_out_explicit_alias_when_canonical_identity_is_ambiguous():
    sentence = ('"Beyond the Clouds, the Promised Place") is a 2004 film written and directed '
                "by Makoto Shinkai in The Place Promised in Our Early Days's feature film debut.")
    assert MODULE.reconcile(sentence, []) == [
        ["Beyond the Clouds, the Promised Place", "director", "Makoto Shinkai"],
        ["Beyond the Clouds, the Promised Place", "screenwriter", "Makoto Shinkai"],
        ["The Place Promised in Our Early Days", "director", "Makoto Shinkai"],
        ["The Place Promised in Our Early Days", "screenwriter", "Makoto Shinkai"],
    ]
