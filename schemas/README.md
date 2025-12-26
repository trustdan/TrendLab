# TrendLab Schemas

This folder contains JSON schemas that define the structure of key data artifacts.

## Files

- `strategy-artifact.schema.json` — Schema for StrategyArtifact exports (used for Pine parity)

## Usage

Validate an artifact against the schema:

```bash
# Using ajv-cli
npx ajv validate -s strategy-artifact.schema.json -d ../artifacts/my-artifact.json

# Using Python jsonschema
python -m jsonschema -i ../artifacts/my-artifact.json strategy-artifact.schema.json
```

## Adding New Schemas

1. Create the schema file following JSON Schema draft-07
2. Add the schema to this README
3. Update `docs/schema.md` if the schema defines a new data structure
