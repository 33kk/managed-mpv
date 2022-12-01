# Managed MPV

A scuffed wrapper for `mpv` to append files to the playlist. Useful for queueing videos to watch from Firefox using `https://add0n.com/external-application-button.html`.

It listens on `$XDG_RUNTIME_DIR/managed-mpv`. When called, a single `mpv` instance will be started, and all urls will be appended to that instance's playlist.

Call the server using `curl` like this:

```bash
curl --unix-socket "/run/user/1000/managed-mpv" "http://localhost/play" -G --data-urlencode "url=$URL" --data-urlencode "title=$TITLE"
```

Hacky way to add titles on YouTube:

```javascript
document.currentScript.output = document.activeElement?.attributes?.["title"]?.value ?? document.activeElement?.attributes?.["aria-label"]?.value ?? document.title;
console.log(`Title: ${document.currentScript.output}`);
```
