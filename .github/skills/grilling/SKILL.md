---
name: grilling
description: Grill the user relentlessly about a plan, decision, or idea. Use when the user wants to stress-test their thinking, or uses any 'grill' trigger phrases.
---

Interview the user until you reach a shared understanding. Map the design as a **decision tree**: every decision branches into the decisions that depend on it.

## Work in rounds

The **frontier** is every decision whose prerequisites are settled—the questions you can ask now without guessing at answers you have not heard yet. Ask the whole frontier in one round, number each question, and give your recommended answer. Then wait for the user's answers before asking the next round.

Format each round like this:

```text
❓ **Q1** - **<question title>**: <question and relevant choices>

➡️ <your recommended answer>

---

❓ **Q2** - **<question title>**: <question and relevant choices>

➡️ <your recommended answer>
```

Word each question so that “yes�� accepts your recommendation. A question that depends on an unsettled decision belongs in a later round, not the current one. After each response, update the decision tree and recompute its frontier.

Finding facts is your job, never the user's. Look up facts in the repository or environment when tools allow; do not ask the user to provide facts you can verify yourself. If a fact is still being investigated, ask the independent questions now and defer only those that depend on that fact.

The session is done when the frontier is empty: every branch of the design tree has been visited and nothing remains silently assumed. Summarize the shared understanding and do not implement or otherwise act on it until the user confirms it.
