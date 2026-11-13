import { describe, it } from 'node:test';
import { RuleTester } from 'eslint';
import { templateParser } from 'angular-eslint';
import { unsafeHtml, unsafeHtmlTemplate } from './unsafe-html.mjs';

RuleTester.describe = describe;
RuleTester.it = it;

const tsTester = new RuleTester({
  languageOptions: { ecmaVersion: 2022, sourceType: 'module' },
});

tsTester.run('unsafe-html', unsafeHtml, {
  valid: [
    'this.sanitizer.sanitize(1, value)',
    'el.textContent = value',
    'el.setAttribute("title", value)',
    'const html = el.innerHTML',
    'writer.write("x")',
    'other.insertAdjacent("x")',
  ],
  invalid: [
    { code: 'this.sanitizer.bypassSecurityTrustUrl(u)', errors: [{ messageId: 'bypass' }] },
    { code: 'sanitizer.bypassSecurityTrustHtml(h)', errors: [{ messageId: 'bypass' }] },
    { code: 'sanitizer["bypassSecurityTrustResourceUrl"](u)', errors: [{ messageId: 'bypass' }] },
    { code: 'el.innerHTML = value', errors: [{ messageId: 'sink' }] },
    { code: 'this.host.nativeElement.outerHTML = value', errors: [{ messageId: 'sink' }] },
    { code: 'el["innerHTML"] = value', errors: [{ messageId: 'sink' }] },
    { code: 'el.insertAdjacentHTML("beforeend", html)', errors: [{ messageId: 'sink' }] },
    { code: 'document.write(html)', errors: [{ messageId: 'sink' }] },
    { code: 'document.writeln(html)', errors: [{ messageId: 'sink' }] },
  ],
});

const templateTester = new RuleTester({
  languageOptions: { parser: templateParser },
});

templateTester.run('unsafe-html-template', unsafeHtmlTemplate, {
  valid: [
    '<div>{{ body }}</div>',
    '<div [textContent]="body"></div>',
    '<div innerHTML="static"></div>',
    '<a [href]="url">x</a>',
  ],
  invalid: [
    {
      code: '<div class="body" [innerHTML]="article.body"></div>',
      errors: [{ messageId: 'binding' }],
    },
    { code: '<div [outerHTML]="html"></div>', errors: [{ messageId: 'binding' }] },
  ],
});
