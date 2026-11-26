#import <AppKit/AppKit.h>
#import <WebKit/WebKit.h>
#include <stdbool.h>
#include <stdint.h>

static NSURL *previewURL(NSString *address) {
    NSCharacterSet *invalid = NSCharacterSet.whitespaceAndNewlineCharacterSet;
    if ([address rangeOfCharacterFromSet:invalid].location != NSNotFound ||
        [address rangeOfCharacterFromSet:NSCharacterSet.controlCharacterSet].location != NSNotFound ||
        [address containsString:@"\\"]) {
        return nil;
    }
    NSURLComponents *parts = [NSURLComponents componentsWithString:address];
    NSString *scheme = parts.scheme.lowercaseString;
    if ((![scheme isEqualToString:@"http"] && ![scheme isEqualToString:@"https"]) ||
        !parts.host.length || parts.user != nil || parts.password != nil) {
        return nil;
    }
    return parts.URL;
}

typedef void (*WorkAnnotationSelected)(const void *, uint64_t, const char *);

static WKContentWorld *annotationWorld(void) {
    return [WKContentWorld worldWithName:@"RPI Work Annotation"];
}

static NSString *annotationScript(NSNumber *token) {
    return [NSString stringWithFormat:@"(() => {"
        "window.workStopAnnotation?.();"
        "const overlay = document.createElement('div');"
        "overlay.style.cssText = 'position:fixed;pointer-events:none;z-index:2147483647;outline:2px solid #477cda;background:#477cda0b;display:none';"
        "document.documentElement.append(overlay);"
        "const target = e => e.target instanceof Element"
        "  ? e.target.closest('h1,h2,h3,h4,p,button,a,label,input,textarea,select,pre,code,img,[role=button]') || e.target"
        "  : null;"
        "const move = e => {"
        "  const node = target(e);"
        "  if (!e.isTrusted || !node) return;"
        "  const r = node.getBoundingClientRect();"
        "  Object.assign(overlay.style, {display:'block',left:r.left+'px',top:r.top+'px',width:r.width+'px',height:r.height+'px'});"
        "};"
        "const hide = () => overlay.style.display = 'none';"
        "const stop = () => {"
        "  overlay.remove();"
        "  window.removeEventListener('pointermove', move, true);"
        "  window.removeEventListener('pointerdown', move, true);"
        "  window.removeEventListener('scroll', hide, true);"
        "  window.removeEventListener('click', pick, true);"
        "  window.removeEventListener('keydown', key, true);"
        "  delete window.workStopAnnotation;"
        "};"
        "const report = payload => {"
        "  stop();"
        "  window.webkit.messageHandlers.workAnnotation.postMessage({token:%@, ...payload});"
        "};"
        "const pick = e => {"
        "  const node = target(e);"
        "  if (!e.isTrusted || !node) return;"
        "  e.preventDefault();"
        "  e.stopImmediatePropagation();"
        "  const editable = 'input,textarea,select,[contenteditable]';"
        "  const text = node.closest(editable) || node.querySelector(editable)"
        "    ? '' : Array.from((node.innerText || '').trim()).slice(0,40).join('');"
        "  report({tag: node.localName, text});"
        "};"
        "const key = e => {"
        "  if (!e.isTrusted || e.key !== 'Escape') return;"
        "  e.preventDefault();"
        "  e.stopImmediatePropagation();"
        "  report({});"
        "};"
        "window.workStopAnnotation = stop;"
        "window.addEventListener('pointermove', move, true);"
        "window.addEventListener('pointerdown', move, true);"
        "window.addEventListener('scroll', hide, true);"
        "window.addEventListener('click', pick, true);"
        "window.addEventListener('keydown', key, true);"
        "})();", token];
}

@interface WorkBrowser : NSView <WKNavigationDelegate, WKUIDelegate, WKScriptMessageHandler>
@property(nonatomic, strong) WKWebView *page;
@property(nonatomic, strong) NSTextField *address;
@property(nonatomic, strong) NSTextField *status;
@property(nonatomic, strong) NSButton *back;
@property(nonatomic, strong) NSButton *forward;
@property(nonatomic, strong) NSButton *reload;
@property(nonatomic, strong) NSButton *annotate;
@property(nonatomic, copy) NSNumber *annotationToken;
@property(nonatomic) uint64_t annotationScope;
@property(nonatomic) uint64_t annotationCounter;
@property(nonatomic) BOOL annotatable;
@property(nonatomic) const void *selectionContext;
@property(nonatomic) WorkAnnotationSelected selectionCallback;
@property(nonatomic, strong) NSView *toolbar;
@property(nonatomic, strong) NSView *footer;
@property(nonatomic, strong) NSButton *openExternally;
@property(nonatomic, strong) NSPopUpButton *viewport;
@property(nonatomic, strong) WKNavigation *navigation;
@end

@implementation WorkBrowser

- (BOOL)isFlipped { return YES; }

- (NSButton *)button:(NSString *)symbol label:(NSString *)label action:(SEL)action {
    NSButton *button = [NSButton buttonWithImage:[NSImage imageWithSystemSymbolName:symbol accessibilityDescription:label]
                                          target:self action:action];
    button.toolTip = label;
    button.bordered = NO;
    [self.toolbar addSubview:button];
    return button;
}

- (instancetype)initWithFrame:(NSRect)frame {
    self = [super initWithFrame:frame];
    if (!self) return nil;
    self.wantsLayer = YES;
    self.layer.masksToBounds = YES;
    self.toolbar = [NSView new];
    self.toolbar.wantsLayer = YES;
    [self addSubview:self.toolbar];
    self.back = [self button:@"chevron.left" label:@"后退" action:@selector(goBack:)];
    self.forward = [self button:@"chevron.right" label:@"前进" action:@selector(goForward:)];
    self.reload = [self button:@"arrow.clockwise" label:@"刷新网页" action:@selector(reloadPage:)];
    self.annotate = [self button:@"bubble.left" label:@"批注" action:@selector(toggleAnnotation:)];
    [self.annotate setButtonType:NSButtonTypePushOnPushOff];
    self.annotate.accessibilityLabel = @"批注";
    self.annotate.font = [NSFont systemFontOfSize:12];
    self.address = [NSTextField textFieldWithString:@""];
    self.address.font = [NSFont monospacedSystemFontOfSize:12 weight:NSFontWeightRegular];
    self.address.placeholderString = @"输入 http:// 或 https:// 地址";
    self.address.accessibilityLabel = @"浏览器地址";
    self.address.target = self;
    self.address.action = @selector(navigate:);
    self.address.cell.sendsActionOnEndEditing = NO;
    self.address.usesSingleLineMode = YES;
    [self.toolbar addSubview:self.address];
    WKWebViewConfiguration *configuration = [WKWebViewConfiguration new];
    configuration.websiteDataStore = WKWebsiteDataStore.nonPersistentDataStore;
    self.page = [[WKWebView alloc] initWithFrame:NSZeroRect configuration:configuration];
    self.page.navigationDelegate = self;
    [configuration.userContentController addScriptMessageHandler:self contentWorld:annotationWorld() name:@"workAnnotation"];
    self.page.UIDelegate = self;
    self.page.hidden = YES;
    [self addSubview:self.page];
    self.status = [NSTextField wrappingLabelWithString:@"输入地址并按 Enter 打开网页。"];
    self.status.textColor = NSColor.secondaryLabelColor;
    [self addSubview:self.status];
    self.footer = [NSView new];
    self.footer.wantsLayer = YES;
    [self addSubview:self.footer];
    self.openExternally = [NSButton buttonWithTitle:@"在新窗口打开" target:self action:@selector(openPageExternally:)];
    self.openExternally.bordered = NO;
    self.openExternally.font = [NSFont systemFontOfSize:11];
    [self.footer addSubview:self.openExternally];
    self.viewport = [[NSPopUpButton alloc] initWithFrame:NSZeroRect pullsDown:NO];
    self.viewport.accessibilityLabel = @"视口";
    self.viewport.controlSize = NSControlSizeSmall;
    self.viewport.font = [NSFont systemFontOfSize:11];
    [self.viewport addItemsWithTitles:@[@"自适应", @"移动设备"]];
    self.viewport.target = self;
    self.viewport.action = @selector(changeViewport:);
    [self.footer addSubview:self.viewport];
    [self updateButtons];
    return self;
}

- (void)layout {
    [super layout];
    CGFloat width = NSWidth(self.bounds);
    CGFloat height = NSHeight(self.bounds);
    self.toolbar.frame = NSMakeRect(0, 0, width, 42);
    self.back.frame = NSMakeRect(6, 7, 26, 28);
    self.forward.frame = NSMakeRect(34, 7, 26, 28);
    self.reload.frame = NSMakeRect(62, 7, 26, 28);
    BOOL showAnnotationTitle = width >= 420;
    CGFloat annotationWidth = showAnnotationTitle ? 56 : 26;
    self.annotate.title = showAnnotationTitle ? @"批注" : @"";
    self.annotate.imagePosition = showAnnotationTitle ? NSImageLeft : NSImageOnly;
    self.address.frame = NSMakeRect(96, 7, MAX(0, width - 110 - annotationWidth), 28);
    self.annotate.frame = NSMakeRect(MAX(0, width - 8 - annotationWidth), 7, annotationWidth, 28);
    self.status.frame = NSMakeRect(18, 60, MAX(0, width - 36), 66);
    self.footer.frame = NSMakeRect(0, MAX(42, height - 29), width, 29);
    self.openExternally.frame = NSMakeRect(5, 2, 98, 25);
    self.viewport.frame = NSMakeRect(MAX(105, width - 94), 3, 86, 23);
    CGFloat viewportWidth = self.viewport.indexOfSelectedItem == 1 ? MIN(width, 390) : width;
    self.page.frame = NSMakeRect((width - viewportWidth) / 2, 42, viewportWidth, MAX(0, height - 71));
}

- (void)updateButtons {
    self.back.enabled = self.page.canGoBack;
    self.forward.enabled = self.page.canGoForward;
    self.reload.enabled = self.page.URL != nil;
    self.openExternally.enabled = self.page.URL != nil;
    self.annotate.enabled = self.annotatable && !self.page.hidden && self.page.URL != nil;
}

- (void)cancelAnnotation {
    if (!self.annotationToken) return;
    self.annotationToken = nil;
    self.annotate.state = NSControlStateValueOff;
    self.annotate.contentTintColor = nil;
    self.annotate.toolTip = @"批注";
    [self.page evaluateJavaScript:@"window.workStopAnnotation?.()" inFrame:nil inContentWorld:annotationWorld() completionHandler:nil];
}

- (void)toggleAnnotation:(id)sender {
    if (self.annotationToken) { [self cancelAnnotation]; return; }
    if (!self.annotate.enabled || self.hidden) return;
    NSNumber *token = @(++self.annotationCounter);
    self.annotationToken = token;
    self.annotate.state = NSControlStateValueOn;
    self.annotate.contentTintColor = NSColor.controlAccentColor;
    self.annotate.toolTip = @"选择要批注的页面元素，或再次点击取消";
    [self.window makeFirstResponder:self.page];
    __weak WorkBrowser *owner = self;
    [self.page evaluateJavaScript:annotationScript(token) inFrame:nil inContentWorld:annotationWorld()
       completionHandler:^(id result, NSError *error) {
        (void)result;
        WorkBrowser *browser = owner;
        if (error && [browser.annotationToken isEqual:token]) [browser cancelAnnotation];
    }];
}

- (void)userContentController:(WKUserContentController *)controller didReceiveScriptMessage:(WKScriptMessage *)message {
    if (self.hidden || self.page.hidden || !self.annotatable || !self.annotationToken ||
        message.webView != self.page || !message.frameInfo.isMainFrame || ![message.body isKindOfClass:NSDictionary.class]) return;
    NSDictionary *body = message.body;
    if (![self.annotationToken isEqual:body[@"token"]]) return;
    NSString *tag = body[@"tag"];
    NSString *text = body[@"text"];
    NSURL *page = previewURL(self.page.URL.absoluteString ?: @"");
    [self cancelAnnotation];
    if (!page || ![tag isKindOfClass:NSString.class] || !tag.length || tag.length > 64 ||
        ![text isKindOfClass:NSString.class] || text.length > 80 || !self.selectionCallback) return;
    NSURLComponents *address = [NSURLComponents componentsWithURL:page resolvingAgainstBaseURL:NO];
    address.query = nil;
    address.fragment = nil;
    NSString *url = address.string;
    if (!url.length || url.length > 1024) return;
    NSString *label = [[text componentsSeparatedByCharactersInSet:NSCharacterSet.controlCharacterSet] componentsJoinedByString:@" "];
    NSString *location = [NSString stringWithFormat:@"%@ · <%@>%@", url, tag, label.length ? [@" · " stringByAppendingString:label] : @""];
    const char *utf8 = location.UTF8String;
    if (utf8) self.selectionCallback(self.selectionContext, self.annotationScope, utf8);
}

- (void)goBack:(id)sender { self.navigation = [self.page goBack]; }
- (void)goForward:(id)sender { self.navigation = [self.page goForward]; }
- (void)reloadPage:(id)sender { self.navigation = [self.page reload]; }

- (void)changeViewport:(id)sender { [self setNeedsLayout:YES]; }

- (void)openPageExternally:(id)sender {
    NSURL *url = previewURL(self.page.URL.absoluteString ?: @"");
    if (url) [NSWorkspace.sharedWorkspace openURL:url];
}

- (void)navigate:(id)sender {
    NSString *text = [self.address.stringValue stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet];
    NSURL *url = previewURL(text);
    if (!url) {
        self.navigation = nil;
        [self cancelAnnotation];
        [self.page stopLoading];
        self.page.hidden = YES;
        self.status.hidden = NO;
        self.status.stringValue = @"请输入不含用户名或密码的完整 HTTP(S) 地址。";
        [self updateButtons];
        return;
    }
    self.navigation = [self.page loadRequest:[NSURLRequest requestWithURL:url]];
}

- (void)webView:(WKWebView *)page decidePolicyForNavigationAction:(WKNavigationAction *)action
 decisionHandler:(void (^)(WKNavigationActionPolicy))decisionHandler {
    if (previewURL(action.request.URL.absoluteString ?: @"")) decisionHandler(WKNavigationActionPolicyAllow);
    else decisionHandler(WKNavigationActionPolicyCancel);
}

- (void)webView:(WKWebView *)page decidePolicyForNavigationResponse:(WKNavigationResponse *)response
 decisionHandler:(void (^)(WKNavigationResponsePolicy))decisionHandler {
    BOOL supported = response.canShowMIMEType && previewURL(response.response.URL.absoluteString ?: @"");
    decisionHandler(supported ? WKNavigationResponsePolicyAllow : WKNavigationResponsePolicyCancel);
}

- (WKWebView *)webView:(WKWebView *)page createWebViewWithConfiguration:(WKWebViewConfiguration *)configuration
 forNavigationAction:(WKNavigationAction *)action windowFeatures:(WKWindowFeatures *)features {
    if (!action.targetFrame && action.navigationType == WKNavigationTypeLinkActivated &&
        previewURL(action.request.URL.absoluteString ?: @"")) {
        self.navigation = [page loadRequest:action.request];
    }
    return nil;
}

- (void)webView:(WKWebView *)page didStartProvisionalNavigation:(WKNavigation *)navigation {
    [self cancelAnnotation];
    self.navigation = navigation;
    self.page.hidden = YES;
    [self updateButtons];
    self.status.hidden = NO;
    self.status.stringValue = @"正在加载网页…";
}

- (void)webView:(WKWebView *)page didFinishNavigation:(WKNavigation *)navigation {
    if (navigation != self.navigation) return;
    self.address.stringValue = page.URL.absoluteString ?: @"";
    self.status.hidden = YES;
    self.page.hidden = NO;
    [self updateButtons];
}

- (void)webView:(WKWebView *)page didFailProvisionalNavigation:(WKNavigation *)navigation withError:(NSError *)error {
    [self webView:page didFailNavigation:navigation withError:error];
}

- (void)webView:(WKWebView *)page didFailNavigation:(WKNavigation *)navigation withError:(NSError *)error {
    if (navigation != self.navigation) return;
    self.page.hidden = YES;
    self.status.hidden = NO;
    self.status.stringValue = error.code == NSURLErrorCancelled ? @"网页导航已取消。" : @"无法加载网页，请检查地址或网络连接。";
    [self updateButtons];
}

- (void)webViewWebContentProcessDidTerminate:(WKWebView *)page {
    [self cancelAnnotation];
    self.page.hidden = YES;
    self.status.hidden = NO;
    self.status.stringValue = @"网页进程已退出。可刷新后重新加载。";
    [self updateButtons];
}

- (void)restoreFocus {
    NSWindow *window = self.window;
    NSResponder *responder = window.firstResponder;
    if (([responder isKindOfClass:NSView.class] &&
         [(NSView *)responder isDescendantOf:self]) || (responder && responder == self.address.currentEditor)) {
        [window makeFirstResponder:self.superview];
    }
}

- (void)hideAndRestoreFocus {
    if (self.hidden) return;
    [self cancelAnnotation];
    [self restoreFocus];
    self.hidden = YES;
}
@end

void *work_browser_create(const void *context, WorkAnnotationSelected callback) {
    WorkBrowser *browser = [[WorkBrowser alloc] initWithFrame:NSZeroRect];
    browser.selectionContext = context;
    browser.selectionCallback = callback;
    return (__bridge_retained void *)browser;
}

void work_browser_mount(void *raw, void *parent, double x, double y, double width, double height, bool dark, uint64_t scope, bool annotatable) {
    WorkBrowser *browser = (__bridge WorkBrowser *)raw;
    NSView *host = (__bridge NSView *)parent;
    if (browser.superview != host) [host addSubview:browser];
    NSRect bounds = host.bounds;
    NSRect frame = NSMakeRect(NSMinX(bounds) + x, host.flipped ? NSMinY(bounds) + y : NSMaxY(bounds) - y - height,
                              width, height);
    if (!NSEqualRects(browser.frame, frame)) browser.frame = frame;
    NSAppearanceName appearance = dark ? NSAppearanceNameDarkAqua : NSAppearanceNameAqua;
    if (![browser.appearance.name isEqualToString:appearance]) {
        browser.appearance = [NSAppearance appearanceNamed:appearance];
        NSColor *background = dark ? [NSColor colorWithWhite:0.12 alpha:1.0] : NSColor.whiteColor;
        browser.layer.backgroundColor = background.CGColor;
        NSColor *controls = dark ? [NSColor colorWithWhite:0.17 alpha:1.0] : [NSColor colorWithWhite:0.95 alpha:1.0];
        browser.toolbar.layer.backgroundColor = controls.CGColor;
        browser.footer.layer.backgroundColor = controls.CGColor;
    }
    if (browser.annotationScope != scope || !annotatable) [browser cancelAnnotation];
    browser.annotationScope = scope;
    browser.annotatable = annotatable;
    browser.hidden = NO;
    [browser updateButtons];
}

void work_browser_restore_focus(void *raw) {
    [(__bridge WorkBrowser *)raw restoreFocus];
}

void work_browser_hide(void *raw) {
    [(__bridge WorkBrowser *)raw hideAndRestoreFocus];
}

void work_browser_destroy(void *raw) {
    WorkBrowser *browser = (__bridge_transfer WorkBrowser *)raw;
    browser.selectionCallback = NULL;
    [browser hideAndRestoreFocus];
    [browser.page.configuration.userContentController removeScriptMessageHandlerForName:@"workAnnotation" contentWorld:annotationWorld()];
    browser.page.navigationDelegate = nil;
    browser.page.UIDelegate = nil;
    [browser.page stopLoading];
    [browser removeFromSuperview];
}
