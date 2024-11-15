import json
import pytest

from extractous import Extractor
from utils import calculate_similarity_percent, cosine_similarity, read_to_string, read_file_to_bytearray

TEST_CASES = [
    ("2022_Q3_AAPL.pdf", 0.9),
    ("science-exploration-1p.pptx", 0.9),
    ("simple.odt", 0.9),
    ("table-multi-row-column-cells-actual.csv", 0.9),
    ("vodafone.xlsx", 0.4),
    ("category-level.docx", 0.9),
    ("simple.doc", 0.9),
    ("simple.pptx", 0.9),
    ("table-multi-row-column-cells.png", -1.0),
    ("winter-sports.epub", 0.9),
    ("bug_16.docx", 0.9),
    #("eng-ocr.pdf", 0.9),
]


@pytest.mark.parametrize("file_name, target_dist", TEST_CASES)
def test_extract_bytes_to_stream(file_name, target_dist):
    """Test the extraction from bytes of various file types."""
    original_filepath = f"../../test_files/documents/{file_name}"
    expected_result_filepath = f"../../test_files/expected_result/{file_name}.txt"

    file_bytes = read_file_to_bytearray(original_filepath)

    extractor = Extractor()
    reader = extractor.extract_bytes(file_bytes)
    result = read_to_string(reader)

    # Expected
    with open(expected_result_filepath, "r",  encoding="utf8") as file:
        expected = file.read()
    
    assert cosine_similarity(result, expected) > target_dist, \
        f"Cosine similarity is less than {target_dist} for file: {file_name}"


TEST_CASES_METADATA = [
    ("2022_Q3_AAPL.pdf", 0.9),
    ("science-exploration-1p.pptx", 0.9),
    ("simple.odt", 0.9),
    ("table-multi-row-column-cells-actual.csv", 0.6),
    ("vodafone.xlsx", 0.8),
    ("category-level.docx", 0.9),
    ("simple.doc", 0.9),
    ("simple.pptx", 0.9),
    ("table-multi-row-column-cells.png", 0.9),
    ("winter-sports.epub", 0.8),
    ("bug_16.docx", 0.9),
]


@pytest.mark.parametrize("file_name, similarity_percent", TEST_CASES_METADATA)
def test_extract_bytes_to_stream(file_name, similarity_percent):
    """Test the extraction from bytes of various file types."""
    original_filepath = f"../../test_files/documents/{file_name}"
    expected_metadata_result_filepath = f"../../test_files/expected_result/{file_name}.metadata.json"
    file_bytes = read_file_to_bytearray(original_filepath)
    extractor = Extractor()
    _reader, metadata = extractor.extract_bytes_with_metadata(file_bytes)
    with open(expected_metadata_result_filepath, 'r', encoding="utf8") as file:
        expected_metadata = json.load(file)
    percent_similarity = calculate_similarity_percent(metadata, expected_metadata)
    assert percent_similarity > similarity_percent, \
        f"The metadata similarity is lower than expected. Current {percent_similarity}% | filename: {file_name}"