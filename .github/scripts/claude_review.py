import os, json, subprocess
import urllib.request

diff = open("/tmp/pr.diff").read()
if not diff.strip():
    print("No diff — skipping review")
    exit(0)

payload = {
    "model": "claude-sonnet-4-6",
    "max_tokens": 2048,
    "messages": [{
        "role": "user",
        "content": f"""Review this Rust PR diff for axiom-engine.
Focus: correctness, safety, Z3/egg/nalgebra usage, atomicity.
Format:
- Issues (file:line — problem — fix)
- Verdict: APPROVED or NEEDS_CHANGES

<diff>
{diff[:15000]}
</diff>"""
    }]
}

req = urllib.request.Request(
    "https://api.anthropic.com/v1/messages",
    data=json.dumps(payload).encode(),
    headers={
        "x-api-key": os.environ["ANTHROPIC_API_KEY"],
        "anthropic-version": "2023-06-01",
        "content-type": "application/json"
    }
)
resp = json.loads(urllib.request.urlopen(req).read())
review = resp["content"][0]["text"]

label = "ai-approved" if "APPROVED" in review else "ai-needs-changes"
pr = os.environ["PR_NUMBER"]
repo = os.environ["REPO"]

subprocess.run(["gh", "pr", "comment", pr, "--repo", repo,
                "--body", f"## Axiom Engine — Claude Code Review\n\n{review}"],
               env={**os.environ})

subprocess.run(["gh", "pr", "edit", pr, "--repo", repo,
                "--add-label", label], env={**os.environ})

print(f"Review posted. Label: {label}")
