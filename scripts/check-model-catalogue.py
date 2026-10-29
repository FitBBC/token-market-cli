#!/usr/bin/env python3
"""Check a redacted /v1/models response captured using a test workspace Key."""
import argparse
import json
import sys
from pathlib import Path

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('file', type=Path, help='Path to the JSON response body; do not include the API Key')
parser.add_argument('--model', default='glm-5.3', help='Required model ID (use glm-5.2 for the test desktop build)')
args = parser.parse_args()
try:
    payload = json.loads(args.file.read_text(encoding='utf-8'))
    rows = payload['data']
    if not isinstance(rows, list) or not rows:
        raise ValueError('data must be a non-empty array')
    for row in rows:
        if not isinstance(row, dict) or not isinstance(row.get('id'), str) or not row['id']:
            raise ValueError('model id missing or invalid')
        if not isinstance(row.get('model_types'), list) or not all(isinstance(x, str) for x in row['model_types']):
            raise ValueError('each model needs a model_types string array')
    glm = next((row for row in rows if row['id'] == args.model), None)
    text_count = sum('TEXT_GENERATION' in row['model_types'] for row in rows)
    if not glm or 'TEXT_GENERATION' not in glm['model_types']:
        raise ValueError(f'{args.model} is not a text-generation model for this Key')
    if not text_count:
        raise ValueError('no text-generation model is available for this Key')
    print(f'Model catalogue OK: {len(rows)} models, {text_count} are text-generation models; {args.model} is available.')
except (OSError, json.JSONDecodeError, KeyError, TypeError, ValueError) as error:
    print(f'Model catalogue check failed: {error}', file=sys.stderr)
    sys.exit(1)
