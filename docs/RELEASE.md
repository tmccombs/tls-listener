Steps to create a new release:

1. Update version in Cargo.toml
2. Run `gi-cliff -p CHANGELOG.md -u -t <new-tag>` to update the changelog
3. Merge changes
4. Publish a new release for the target tag.

TODO: Create github action to do all of this.
