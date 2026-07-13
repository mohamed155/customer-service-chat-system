import { provideZonelessChangeDetection } from '@angular/core';
import { TestBed } from '@angular/core/testing';
import { of } from 'rxjs';
import { Customer } from '../../../core/api/tenant-api.models';
import { ApiService } from '../../../core/api/api.service';
import { CustomersApiService } from '../customers/customers-api.service';
import { ConversationsApiService } from './conversations-api.service';
import { NewConversationDialogComponent } from './new-conversation-dialog.component';

describe('NewConversationDialogComponent', () => {
  const mockCustomers: Customer[] = [
    {
      id: 'cu1',
      displayName: 'Maya Chen',
      email: 'maya@example.com',
      phone: null,
      channels: ['email', 'web_chat'],
      createdAt: '2026-07-01T00:00:00Z',
      updatedAt: '2026-07-10T00:00:00Z',
    },
  ];

  let apiService: { get: ReturnType<typeof vi.fn>; post: ReturnType<typeof vi.fn> };
  let convApi: { create: ReturnType<typeof vi.fn> };

  function createComponent() {
    TestBed.configureTestingModule({
      imports: [NewConversationDialogComponent],
      providers: [
        provideZonelessChangeDetection(),
        CustomersApiService,
        { provide: ApiService, useValue: apiService },
        { provide: ConversationsApiService, useValue: convApi },
      ],
    });
    const fixture = TestBed.createComponent(NewConversationDialogComponent);
    fixture.detectChanges();
    return { fixture, component: fixture.componentInstance };
  }

  beforeEach(() => {
    apiService = { get: vi.fn(), post: vi.fn() };
    convApi = { create: vi.fn() };
    apiService.get.mockReturnValue(
      of({
        data: { data: mockCustomers, pagination: { next_cursor: null, has_more: false } },
      }),
    );
  });

  it('renders the dialog with form fields', () => {
    const { fixture } = createComponent();
    const el = fixture.nativeElement;
    expect(el.textContent).toContain('New conversation');
    expect(el.querySelector('input[aria-label="Search customers"]')).toBeTruthy();
    expect(el.querySelector('select[aria-label="Channel"]')).toBeTruthy();
    expect(el.querySelector('textarea[aria-label="First message"]')).toBeTruthy();
    expect(el.textContent).toContain('Cancel');
    expect(el.textContent).toContain('Create conversation');
  });

  it('shows validation errors when submitting with empty fields', () => {
    const { component, fixture } = createComponent();
    const createSpy = vi.fn();
    component.create.subscribe(createSpy);

    const submitBtn = fixture.nativeElement.querySelector('button[type="submit"]');
    submitBtn.click();
    fixture.detectChanges();

    expect(createSpy).not.toHaveBeenCalled();
  });

  it('shows required error for empty channel', () => {
    const { component } = createComponent();
    const channelControl = component['form'].controls.channel;
    channelControl.markAsTouched();
    channelControl.setValue('');

    expect(channelControl.invalid).toBe(true);
  });

  it('shows required error for empty message body', () => {
    const { component, fixture } = createComponent();
    const bodyControl = component['form'].controls.body;
    bodyControl.markAsTouched();
    bodyControl.setValue('');
    fixture.detectChanges();

    expect(bodyControl.invalid).toBe(true);
    expect(fixture.nativeElement.textContent).toContain('Message is required');
  });

  it('submits with valid form and emits created conversation id', async () => {
    convApi.create.mockReturnValue(of({ data: { id: 'conv-new' } }));

    const { component, fixture } = createComponent();
    const createSpy = vi.fn();
    component.create.subscribe(createSpy);

    component['selectedCustomer'].set(mockCustomers[0]);
    component['form'].controls.customerSearch.setValue(mockCustomers[0].displayName);
    component['form'].controls.channel.setValue('web_chat');
    component['form'].controls.body.setValue('Hello, I need help');
    fixture.detectChanges();

    component['submit']();

    await vi.waitFor(() => {
      expect(convApi.create).toHaveBeenCalledWith({
        customerId: 'cu1',
        channel: 'web_chat',
        message: { body: 'Hello, I need help' },
      });
      expect(createSpy).toHaveBeenCalledWith('conv-new');
    });
  });

  it('disables submit button while submitting', () => {
    const { component, fixture } = createComponent();
    component['submitting'].set(true);
    fixture.detectChanges();

    const submitBtn = fixture.nativeElement.querySelector('button[type="submit"]');
    expect(submitBtn.disabled).toBe(true);
    expect(submitBtn.textContent).toContain('Creating');
  });

  it('emits closeDialog when cancel is clicked', () => {
    const { component } = createComponent();
    const closeSpy = vi.fn();
    component.closeDialog.subscribe(closeSpy);

    component.closeDialog.emit();
    expect(closeSpy).toHaveBeenCalled();
  });
});
