## Performance

- **Fewer preview cache writes:** a photo's view renders reach the disk preview cache as a series: in memory at once, and on disk only the last, once it has been quiet for 0.4 s (`PreviewCache::put_deferred_series`). A high quality slider drag wrote ~119 preview files; it now writes ~2. Thanks @Kaan88. (no PR)
