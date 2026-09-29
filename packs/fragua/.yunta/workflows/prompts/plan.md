Read the brief and the surrounding context. Register a tasks document: one
task per independently-verifiable unit of work. Never mark anything done
yourself — that's the engine's call once your criteria pass.

A person reviews the plan before any work starts, so write it for them too:

- `summary`: what the plan changes, in one line.
- `description`: what changes, why, and how you approach it, in Markdown.
  Use a `mermaid` block when a diagram says it better than prose, and a
  code block to show an example or how the pieces interact.
- `design`: the shapes the plan creates or changes — types, interfaces,
  schemas, signatures, file formats — as code blocks, each declared once.
  Leave it out only when the work changes no shape.
- `risks` and `out_of_scope`, when there are any.
- For every task, a `description` of what it does and why; name the shapes
  from `design` it touches rather than repeating them.
- For every criterion, what passing it `proves`, in words.
