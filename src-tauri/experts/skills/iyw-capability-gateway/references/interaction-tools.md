# Direct user interaction

Choose by what helps the user, not by task size. Call the actual advertised
identity directly; neither interaction tool requires capability discovery.

| Tool | Use when | Example |
| --- | --- | --- |
| `ask_user_question` | A concise answer, preference or user-owned decision is needed | Clarify scope, select a target, choose an approach, supply a missing field |
| `show_interactive_html` | Seeing and interacting helps understanding, exploration or feedback | Simulation, interactive explanation, visual comparison, design preview, ordering, annotation, custom mini-tool |

These examples do not limit what you can build. Design the page's HTML, CSS,
JavaScript, SVG, Canvas, layout, visual style and interaction logic freely. No
fixed form schema or component library is required. The user need not ask for HTML.
Avoid redundant question cards when the page already collects the same choice.

## Questions

Provide `questions` with one to four entries. Only `question` is required per
entry. `header` is optional (up to 12 characters), `multiSelect` defaults to false,
and `options` is optional (up to four label/description pairs). Omit options for
an open-ended question. The host always provides multiline free text, so do not
add an Other option. Put a recommended choice first and label it `(Recommended)`;
the user still makes an explicit choice and submits. The call waits for an answer
or dismissal. Do not ask routine “should I continue?” questions or collect secrets.

## HTML presentation and feedback

Supply `title` (up to 120 characters) and a complete `html` document (up to 256 KiB
UTF-8). Inline scripts/styles and embedded data/blob assets work automatically.
External downloads, host credentials, filesystem operations and tool calls are
not available inside the page. Make the page responsive and keyboard accessible.
Use actual task data or label illustrative data clearly.

`wait_for_response` defaults to false: show the page, let the user explore and
continue your work. The result `presented` means the host accepted the document;
it is not evidence that the browser finished rendering. Display pages remain
available after the tool returns, and do not send results back to the agent.

Set `wait_for_response: true` when you need feedback before the next step. The
host injects `iyw.submit(data)`, which returns a Promise. `data` can be any JSON
value up to 64 KiB, with fields you design for the task. Bind submission to a
clear user action, such as “Use this design” or “Finish annotation”. Do not submit
on page load. Keep intermediate experimentation local, prevent double submission,
and display retryable errors without resetting the user's work.

```javascript
document.querySelector('#finish').addEventListener('click', async (event) => {
  const button = event.currentTarget
  button.disabled = true
  try {
    await iyw.submit({ selected: selectedItems, parameters: currentParameters })
  } catch (error) {
    document.querySelector('#status').textContent = error.message
    button.disabled = false
  }
})
```

This fragment illustrates the bridge; choose your own interaction and complete
page. Results distinguish `submitted` from `cancelled`; a fallback text reply has
`source: "text"`. Do not treat cancellation as a choice or approval. There is one
pending question/HTML collection per session and up to eight open HTML pages.
On reconnect the page can reload and unsent local edits may reset. Historical
previews are opened explicitly and cannot submit to the old request.
