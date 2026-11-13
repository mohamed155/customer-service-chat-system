// Review gate for the places that can write HTML into the page (MOH-170, architecture §13.2).
//
// The dashboard runs under Trusted Types (`require-trusted-types-for 'script'`), so a sink fed an
// unsafe string is blocked in the browser. This rule is the earlier line: it makes the code that
// reaches a sink, or turns Angular's sanitiser off, stand out in review. It is not a ban. A real
// need is allowed with a disable comment that says why it is safe:
//
//   // eslint-disable-next-line html-safety/unsafe-html -- the body is sanitised by the server (ammonia)
//
// In a template: `<!-- eslint-disable-next-line html-safety/unsafe-html-template -- reason -->`.
//
// TypeScript (`unsafeHtml`): `bypassSecurityTrust*()`, assignment to `innerHTML` / `outerHTML`,
// `insertAdjacentHTML()` and `document.write()` / `writeln()`.
// Templates (`unsafeHtmlTemplate`): `[innerHTML]` and `[outerHTML]` bindings.

const SINK_PROPERTIES = new Set(['innerHTML', 'outerHTML']);
const SINK_METHODS = new Set(['insertAdjacentHTML']);
const DOCUMENT_WRITERS = new Set(['write', 'writeln']);

const propertyName = (member) =>
  member.computed
    ? member.property.type === 'Literal'
      ? String(member.property.value)
      : null
    : member.property.name;

/** @type {import('eslint').Rule.RuleModule} */
export const unsafeHtml = {
  meta: {
    type: 'problem',
    schema: [],
    messages: {
      bypass:
        '{{name}}() turns off Angular’s sanitiser. Allow it with a disable comment that says why the value is safe.',
      sink: 'Writing HTML with {{name}} needs review (Trusted Types blocks unsafe strings here). Use a template, or allow it with a disable comment that says why it is safe.',
    },
  },
  create(context) {
    return {
      CallExpression(node) {
        const callee = node.callee;
        if (callee.type !== 'MemberExpression') return;
        const name = propertyName(callee);
        if (name === null) return;
        if (name.startsWith('bypassSecurityTrust')) {
          context.report({ node, messageId: 'bypass', data: { name } });
        } else if (SINK_METHODS.has(name)) {
          context.report({ node, messageId: 'sink', data: { name } });
        } else if (
          DOCUMENT_WRITERS.has(name) &&
          callee.object.type === 'Identifier' &&
          callee.object.name === 'document'
        ) {
          context.report({ node, messageId: 'sink', data: { name: `document.${name}` } });
        }
      },
      AssignmentExpression(node) {
        if (node.left.type !== 'MemberExpression') return;
        const name = propertyName(node.left);
        if (name !== null && SINK_PROPERTIES.has(name)) {
          context.report({ node, messageId: 'sink', data: { name } });
        }
      },
    };
  },
};

/** @type {import('eslint').Rule.RuleModule} */
export const unsafeHtmlTemplate = {
  meta: {
    type: 'problem',
    schema: [],
    messages: {
      binding:
        '[{{name}}] writes HTML into the page. Angular sanitises it, but it needs review: allow it with a disable comment that says why the value is safe.',
    },
  },
  create(context) {
    return {
      BoundAttribute(node) {
        if (SINK_PROPERTIES.has(node.name)) {
          context.report({ loc: node.loc, messageId: 'binding', data: { name: node.name } });
        }
      },
    };
  },
};

export default {
  rules: { 'unsafe-html': unsafeHtml, 'unsafe-html-template': unsafeHtmlTemplate },
};
