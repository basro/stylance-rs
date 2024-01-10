build-docs:
	RUSTDOCFLAGS="--cfg docsrs" cargo doc -p stylance -p stylance-cli --all-features --open