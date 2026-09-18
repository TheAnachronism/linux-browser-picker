# Target Routing

This context describes how incoming web links and local files are assigned to an installed browser destination, either automatically where allowed or through a user choice.

## Language

**Browser Picker**:
The desktop product containing the Target Router, Picker, and configuration interface.
_Avoid_: Linux Browser Picker, URL Router

**Target Router**:
The stateless Browser Picker role that receives Open Targets and resolves how each should be opened.
_Avoid_: URL router, browser manager, browser launcher, persistent agent

**Open Target**:
One HTTP or HTTPS URL or one existing regular local file submitted for routing. Local files always require a Picker choice; only web URLs may be resolved automatically.
_Avoid_: URL, item, resource

**Browser Application**:
An installed desktop application that can accept Open Targets, discovered from HTTP and HTTPS desktop registration or defined by an executable and structured argument template.
_Avoid_: Browser executable, system browser

**Browser Profile**:
A browser-owned user context that may distinguish one Browser Destination from another using the same Browser Application. Browser Picker discovers existing profiles but never creates, renames, or deletes them.
_Avoid_: Account, session

**Browser Candidate**:
A discovered Browser Application or Browser Profile available during setup but absent from the Picker until the user enables it as a Browser Destination.
_Avoid_: Detected destination, automatic destination

**Browser Destination**:
A Browser Application paired with an optional Browser Profile. It is the selectable endpoint for an Open Target.
_Avoid_: Browser, handler, target

**Routing Rule**:
A named, disableable item in the ordered rule list containing a boolean expression of URL Conditions and either an automatic open action or a Preselection. Routing Rules apply only to web URLs; the first enabled match wins.
_Avoid_: Default rule, filter, association

**URL Condition**:
A predicate over the stable Matching URL. Scheme and host comparisons ignore case; other structured comparisons follow URL case semantics by default but may explicitly ignore case.
_Avoid_: Matcher, clause

**Matching URL**:
The stable representation of an incoming URL used by Routing Rules. Its scheme and internationalized host are lowercased, its host uses ASCII form, and its explicit default port is removed; its path, query, and fragment retain their original spelling and order.
_Avoid_: Canonical URL, raw URL

**Preselection**:
A Routing Rule action that opens the Picker with a Browser Destination and Launch Mode suggested but still requires user confirmation.
_Avoid_: Soft default, recommendation

**Fallback Action**:
The behavior used when no Routing Rule matches a web URL: either show the Picker or open a configured Browser Destination. It never applies to local files.
_Avoid_: Default browser, catch-all rule

**Launch Mode**:
The normal or browser-native private/incognito context used to open an Open Target. It belongs to a one-time choice, Routing Rule action, or Fallback Action and does not promise a separate process, isolated profile, or absence of disk traces.
_Avoid_: Private destination, incognito browser

**Pending Request**:
One accepted Open Target awaiting a user choice. Pending Requests remain distinct and are presented sequentially in one Picker. A synchronous automatic dispatch failure turns the Open Target into a Pending Request with the action's Browser Destination and Launch Mode as its Preselection; choosing another destination must not weaken that Launch Mode. A configuration save may re-evaluate a Pending Request's Preselection but cannot dispatch it; newly accepted web URLs may still resolve automatically.
_Avoid_: URL batch, tab group

**Picker**:
The keyboard-first, pointer-accessible interface for choosing a Browser Destination and Launch Mode for one Open Target.
_Avoid_: Quick switch, browser chooser
