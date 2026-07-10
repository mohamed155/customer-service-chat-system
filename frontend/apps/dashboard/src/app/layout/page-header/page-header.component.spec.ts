import { TestBed } from '@angular/core/testing';
import { PageHeaderComponent } from './page-header.component';

describe('PageHeaderComponent', () => {
  it('renders an in-page heading without adding a second header landmark', () => {
    const fixture = TestBed.createComponent(PageHeaderComponent);
    fixture.componentRef.setInput('title', 'Section title');
    fixture.detectChanges();

    const element: HTMLElement = fixture.nativeElement;
    expect(element.querySelector('h1')?.textContent).toContain('Section title');
    expect(element.querySelector('header')).toBeNull();
  });

  it('renders with title only (no description)', () => {
    const fixture = TestBed.createComponent(PageHeaderComponent);
    fixture.componentRef.setInput('title', 'Section title');
    fixture.detectChanges();

    const element: HTMLElement = fixture.nativeElement;
    expect(element.querySelector('h1')?.textContent).toContain('Section title');
    expect(element.querySelector('.description')).toBeNull();
  });

  it('renders description when provided', () => {
    const fixture = TestBed.createComponent(PageHeaderComponent);
    fixture.componentRef.setInput('title', 'Section title');
    fixture.componentRef.setInput('description', 'A short description');
    fixture.detectChanges();

    const element: HTMLElement = fixture.nativeElement;
    expect(element.querySelector('.description')?.textContent).toContain('A short description');
  });

  it('renders description text in the DOM', () => {
    const fixture = TestBed.createComponent(PageHeaderComponent);
    fixture.componentRef.setInput('title', 'Section title');
    fixture.componentRef.setInput('description', 'Detailed description here');
    fixture.detectChanges();

    const element: HTMLElement = fixture.nativeElement;
    const descEl = element.querySelector<HTMLElement>('.description');
    expect(descEl).not.toBeNull();
    expect(descEl!.textContent).toContain('Detailed description here');
    expect(descEl!.tagName).toBe('P');
  });
});
