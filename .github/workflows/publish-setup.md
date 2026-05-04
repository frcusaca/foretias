# PyPI Trusted Publisher Setup

After merging to the default branch, configure the trusted publisher:

1. Go to https://pypi.org/project/foretias+/settings/
2. Scroll to "Trusted Publishers" and click "Add an item"
3. Select "GitHub" as the integration
4. Set the repository to `frcusaca/foretias`
5. Click "Save"

This links your GitHub repo to PyPI — when a `v*` tag is pushed, PyPI
trusts the GitHub Actions run and publishes the package without any API
token.

Alternatively, for token-based publishing:
1. Generate a PyPI API token at https://pypi.org/manage/account/token/
2. Add it as a GitHub secret: `PYPI_API_TOKEN`
3. The workflow already references this secret
