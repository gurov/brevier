# Responsible JavaScript: Part II

Jeremy Wagner

You and the rest of the dev team lobbied enthusiastically for a total re-architecture of the company’s aging website. Your pleas were heard by management—even up to the C-suite—who gave the green light. Elated, you and the team started working with the design, copy, and IA teams. Before long, you were banging out new code.

Article Continues Below

It started out innocently enough with an `npm install` here and an `npm install` there. Before you knew it, though, you were installing production dependencies like an undergrad doing keg stands without a care for the morning after.

Then you launched.

Unlike the aftermath of most copious boozings, the agony didn’t start the morning after. *Oh*, no. It came months later in the ghastly form of low-grade nausea and headache of product owners and middle management wondering why conversions and revenue were both down since the launch. It then hit a fever pitch when the CTO came back from a weekend at the cabin and wondered why the site loaded so slowly on their phone—if it indeed ever loaded at all.

Everyone was happy. Now *no* one is happy. Welcome to your first JavaScript hangover.

## It’s not your fault

When you’re grappling with a vicious hangover, “I told you so” would be a well-deserved, if fight-provoking, rebuke—assuming you could even fight in so sorry a state.

When it comes to JavaScript hangovers, there’s plenty of blame to dole out. Pointing fingers is a waste of time, though. The landscape of the web today demands that we iterate faster than our competitors. This kind of pressure means we’re likely to take advantage of any means available to be as productive as possible. *That* means we’re more likely—but not necessarily doomed—to build apps with more overhead, and possibly use patterns that can hurt performance and accessibility.

Web development isn’t easy. It’s a long slog we rarely get right on the first try. The best part of working on the web, however, is that we don’t *have* to get it perfect at the start. We can make improvements after the fact, and that’s just what the second installment of [this series](https://alistapart.com/article/responsible-javascript-part-1/) is here for. Perfection is a long ways off. For now, let’s take the edge off of that JavaScript hangover by improving your site’s, er, *scriptuation* in the short term.

## Round up the usual suspects

It might seem rote, but it’s worth going through the list of basic optimizations. It’s not uncommon for large development teams—particularly those that work across many repositories or don’t use optimized boilerplate—to overlook them.

### Shake those trees

First, make sure your toolchain is configured to perform [*tree shaking*](https://developer.mozilla.org/en-US/docs/Glossary/Tree_shaking). If tree shaking is new to you, I wrote [a guide on it last year](https://developers.google.com/web/fundamentals/performance/optimizing-javascript/tree-shaking/) you can consult. The short of it is that tree shaking is a process in which unused exports in your codebase don’t get packaged up in your production bundles.

Tree shaking is available out of the box with modern bundlers such as [webpack](https://webpack.js.org/), [Rollup](https://rollupjs.org/), or [Parcel](https://parceljs.org/). [Grunt](https://gruntjs.com/) or [gulp](https://gulpjs.com/)—which are not *bundlers*, but rather *task runners*—won’t do this for you. A task runner doesn’t build a [dependency graph](https://webpack.js.org/concepts/dependency-graph/) like a bundler does. Rather, they perform discrete tasks on the files you feed to them with any number of plugins. Task runners *can* be extended with plugins to use bundlers to process JavaScript. If extending task runners in this way is problematic for you, you’ll likely need to manually audit and remove unused code.

For tree shaking to be effective, the following must be true:

1. Your app logic and the packages you install in your project must be authored as [ES6 modules](https://ponyfoo.com/articles/es6-modules-in-depth). Tree shaking [CommonJS](https://en.wikipedia.org/wiki/CommonJS) modules isn’t practically possible.
2. Your bundler must *not* transform ES6 modules into another module format at build time. If this happens in a toolchain that uses Babel, [@babel/preset-env configuration](https://babeljs.io/docs/en/babel-preset-env) must specify [`modules: false`](https://babeljs.io/docs/en/babel-preset-env#modules) to prevent ES6 code from being converted to CommonJS.

On the off chance tree shaking isn’t occurring during your build, getting it to work may help. Of course, its effectiveness varies on a case-by-case basis. It also depends on whether the modules you import introduce [side effects](https://en.wikipedia.org/wiki/Side_effect_\(computer_science\)), which may influence a bundler’s ability to shake unused exports.

### Split that code

Chances are good that you’re employing some form of [code splitting](https://developers.google.com/web/fundamentals/performance/optimizing-javascript/code-splitting/), but it’s worth re-evaluating how you’re doing it. No matter *how* you’re splitting code, there are two questions that are always worth asking yourself:

1. Are you [deduplicating common code](https://developers.google.com/web/fundamentals/performance/optimizing-javascript/code-splitting/#removing_duplicate_code) between [entry points](https://webpack.js.org/concepts/entry-points/)?
2. Are you lazy loading all the functionality you reasonably can with [dynamic `import()`](https://developers.google.com/web/updates/2017/11/dynamic-import)?

These are important because reducing redundant code is essential to performance. Lazy loading functionality also improves performance by lowering the initial JavaScript footprint on a given page. On the redundancy front, using an analysis tool such as [Bundle Buddy](https://github.com/samccone/bundle-buddy) can help you find out if you have a problem.

![The Bundle Buddy utility demonstrating how much code is shared between bundles of JavaScript.](https://i0.wp.com/alistapart.com/wp-content/uploads/2019/06/figure-6-2x.png?resize=652%2C628&ssl=1)

Bundle Buddy can examine your webpack compilation statistics and determine how much code is shared between your bundles.

Where lazy loading is concerned, it can be a bit difficult to know where to start looking for opportunities. When I look for opportunities in existing projects, I’ll search for user interaction points throughout the codebase, such as click and keyboard events, and similar candidates. Any code that requires a user interaction to run is a potentially good candidate for dynamic `import()`.

Of course, loading scripts on demand brings the possibility that interactivity could be noticeably delayed, as the script necessary for the interaction must be downloaded first. If data usage is not a concern, consider using the [`rel=prefetch` resource hint](https://www.w3.org/TR/resource-hints/#prefetch) to load such scripts at a low priority that won’t contend for bandwidth against critical resources. [Support for`rel=prefetch`](https://caniuse.com/#feat=link-rel-prefetch) is good, but nothing will break if it’s unsupported, as such browsers will ignore markup they doesn’t understand.

### Externalize third-party hosted code

Ideally, you should self-host as many of your site’s dependencies as possible. If for some reason you *must* load dependencies from a third party, [mark them as externals](https://webpack.js.org/configuration/externals/) in your bundler’s configuration. Failing to do so could mean your website’s visitors will download both locally hosted code *and* the same code from a third party.

Let’s look at a hypothetical situation where this could hurt you: say that your site loads Lodash from a public CDN. You’ve also installed Lodash in your project for local development. However, if you fail to mark Lodash as external, your production code will end up loading a third party copy of it *in addition* to the bundled, locally hosted copy.

This may *seem* like common knowledge if you know your way around bundlers, but I’ve seen it get overlooked. It’s worth your time to check twice.

If you aren’t convinced to self-host your third-party dependencies, then consider adding [`dns-prefetch`](https://css-tricks.com/prefetching-preloading-prebrowsing/#article-header-id-0), [`preconnect`](https://css-tricks.com/prefetching-preloading-prebrowsing/#article-header-id-1), or possibly even [`preload`](https://www.smashingmagazine.com/2016/02/preload-what-is-it-good-for/) hints for them. Doing so can lower your site’s [Time to Interactive](https://developers.google.com/web/tools/lighthouse/audits/time-to-interactive) and—if JavaScript is critical to rendering content—your site’s [Speed Index](https://sites.google.com/a/webpagetest.org/docs/using-webpagetest/metrics/speed-index).

## Smaller alternatives for less overhead

[Userland JavaScript](https://nodejs.org/en/knowledge/getting-started/what-is-node-core-verus-userland/) is like an obscenely massive candy store, and we as developers are awed by the sheer amount of open source offerings. Frameworks and libraries allow us to extend our applications to quickly do all sorts of stuff that would otherwise take loads of time and effort.

While I personally prefer to aggressively minimize the use of client-side frameworks and libraries in my projects, their value is compelling. Yet, we *do* have a responsibility to be a bit hawkish when it comes to what we install. When we’ve already built and shipped something that depends on a slew of installed code to run, we’ve accepted a baseline cost that only the maintainers of that code can practically address. Right?

Maybe, but then again, maybe not. It depends on the dependencies used. For instance, React is extremely popular, but [Preact](https://preactjs.com/) is an [ultra-small](https://bundlephobia.com/result?p=preact@8.4.2) alternative that largely shares the same API and retains compatibility with many React add-ons. [Luxon](https://moment.github.io/luxon/) and [date-fns](https://date-fns.org/) are much more compact alternatives to [moment.js](https://momentjs.com/), which is [not exactly tiny](https://bundlephobia.com/result?p=moment).

Libraries such as [Lodash](https://lodash.com/) offer many useful methods. Yet, some of them are easily replaceable with native ES6. [Lodash’s `compact` method](https://lodash.com/docs/4.17.11#compact), for example, is replaceable with the [`filter` array method](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Array/filter). [Many more can be replaced](https://github.com/you-dont-need/You-Dont-Need-Lodash-Underscore#_chunk) without much effort, and without the need for pulling in a large utility library.

Whatever your preferred tools are, the idea is the same: do some research to see if there are smaller alternatives, or if native language features can do the trick. You may be surprised at how little effort it may take you to seriously reduce your app’s overhead.

## Differentially serve your scripts

There’s a good chance you’re using Babel in your toolchain to transform your ES6 source into code that can run on older browsers. Does this mean we’re doomed to serve giant bundles even to browsers that don’t need them, until the older browsers disappear altogether? [Of course not](https://philipwalton.com/articles/deploying-es2015-code-in-production-today/)! Differential serving helps us get around this by generating two different builds of your ES6 source:

- Bundle one, which contains all the transforms and polyfills required for your site to work on older browsers. You’re probably already serving this bundle right now.
- Bundle two, which contains *little to none* of the transforms and polyfills because it targets modern browsers. This is the bundle you’re probably not serving—at least not *yet*.

Achieving this is a bit involved. [I’ve written a guide on one way you can do it](https://calendar.perfplanet.com/2018/doing-differential-serving-in-2019/), so there’s no need for a deep dive here. The long and short of it is that you can modify your build configuration to generate an additional but smaller version of your site’s JavaScript code, and serve it only to modern browsers. The best part is that these are savings you can achieve without sacrificing any features or functionality you already offer. Depending on your application code, the savings could be quite significant.

![](https://i0.wp.com/alistapart.com/wp-content/uploads/2019/06/diff-serving-bundles.jpg?resize=960%2C297&ssl=1)

A webpack-bundle-analyzer analysis of a project’s legacy bundle (left) versus one for a modern bundle (right). [View full-sized image](https://alistapart.com/wp-content/uploads/2019/06/diff-serving-bundles.jpg).

The [simplest pattern](https://developers.google.com/web/fundamentals/primers/modules#browser) for serving these bundles to their respective platforms is brief. It also works a treat in modern browsers:

```html
<!-- Modern browsers load this file: -->
[/js/app.mjs](https://alistapart.com/js/app.mjs)
<!-- Legacy browsers load this file: -->
[/js/app.js](https://alistapart.com/js/app.js)
```

Unfortunately, there’s a caveat with this pattern: legacy browsers like IE 11—and even relatively modern ones such as Edge versions 15 through 18—will download *both* bundles. If this is an acceptable trade-off for you, then worry no further.

On the other hand, you’ll need a workaround if you’re concerned about [the performance implications of older browsers downloading both sets of bundles](https://gist.github.com/jakub-g/5fc11af85a061ca29cc84892f1059fec). Here’s one potential solution that uses script injection (instead of the `script` tags above) to avoid double downloads on affected browsers:

```javascript
var scriptEl = document.createElement("script");

if ("noModule" in scriptEl) {
  // Set up modern script
  scriptEl.src = "/js/app.mjs";
  scriptEl.type = "module";
} else {
  // Set up legacy script
  scriptEl.src = "/js/app.js";
  scriptEl.defer = true; // type="module" defers by default, so set it here.
}

// Inject!
document.body.appendChild(scriptEl);
```

This script infers that if a browser supports [the `nomodule` attribute](https://developer.mozilla.org/en-US/docs/Web/HTML/Element/script#attr-nomodule) in the `script` element, it understands `type="module"`. This ensures that legacy browsers only get legacy scripts and modern browsers only get modern ones. Be warned, though, that dynamically injected scripts load asynchronously by default, so set the `[async](https://developer.mozilla.org/en-US/docs/Web/HTML/Element/script#attr-async)` attribute to `false` if dependency order is crucial.

## Transpile less

I’m not here to trash Babel. It’s indispensable, but lordy, it adds a *lot* of extra stuff without your ever knowing. It pays to peek under the hood to see what it’s up to. Some minor changes in your coding habits can have a positive impact on what Babel spits out.

![](https://i0.wp.com/alistapart.com/wp-content/uploads/2019/06/twete.png?resize=758%2C421&ssl=1)

[https://twitter.com/\_developit/status/1110229993999777793](https://twitter.com/_developit/status/1110229993999777793)

To wit: [default parameters](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Functions/Default_parameters) are a *very* handy ES6 feature you probably already use:

```javascript
function logger(message, level = "log") {
  console[level](message);
}
```

The thing to pay attention to here is the `level` parameter, which has a default of “log.” This means if we want to invoke `console.log` with this wrapper function, we don’t need to specify `level`. Great, right? Except when Babel transforms this function, the output looks like this:

```javascript
function logger(message) {
  var level = arguments.length > 1 && arguments[1] !== undefined ? arguments[1] : "log";

  console[level](message);
}
```

This is an example of how, despite our best intentions, developer conveniences can backfire. What was a handful of bytes in our source has now been transformed into *much* larger in our production code. Uglification can’t do much about it either, as arguments can’t be reduced. Oh, and if you think [rest parameters](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Functions/rest_parameters) might be a worthy antidote, Babel’s transforms for them are even bulkier:

```javascript
// Source
function logger(...args) {
  const [level, message] = args;

  console[level](message);
}

// Babel output
function logger() {
  for (var _len = arguments.length, args = new Array(_len), _key = 0; _key < _len; _key++) {
    args[_key] = arguments[_key];
  }

  const level = args[0],
        message = args[1];
  console[level](message);
}
```

Worse yet, Babel transforms this code even for projects with a [@babel/preset-env](https://babeljs.io/docs/en/babel-preset-env) configuration [targeting modern browsers](https://babeljs.io/docs/en/babel-preset-env#targetsesmodules), meaning the modern bundles in your differentially served JavaScript will be affected too! You *could* use [loose transforms](https://babeljs.io/docs/en/babel-preset-env#loose) to soften the blow—and that’s a fine idea, as they’re often quite a bit smaller than their more spec-compliant counterparts—[but enabling loose transforms can cause issues if you remove Babel from your build pipeline later on](http://2ality.com/2015/12/babel6-loose-mode.html).

Regardless of whether you decide to enable loose transforms, here’s one way to cut the cruft of transpiled default parameters:

````javascript
```javascript
// Babel won't touch this
function logger(message, level) {
  console[level || "log"](message);
}
```
````

Of course, default parameters aren’t the *only* feature to be wary of. For example, [spread syntax](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Operators/Spread_syntax) gets transformed, as do [arrow functions](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Functions/Arrow_functions) and a whole host of [other stuff](https://babeljs.io/repl/#?babili=false&browsers=%3E%200.25%25%2C%20ie%20%3E%2010%2C%20Firefox%20ESR%2C%20not%20dead&build=&builtIns=false&spec=false&loose=false&code_lz=MYGwhgzhAECyYDsCuAzMwAuSBOBTb0A3gFDTTAD2CEG2SmFBAFALaKrpZ7YA05FSBLQCeASiKky0DAAsAlhAB0bZGkw580ALzQVHddwDcksrIWLKgkdv5Xsw42QC-xF8VCQYAYTAFcADwxcBAATGHhVTg0CEjJKalp6DEZoVgoQ3BA-YVxfPkoQRj5FEt8AcwhxWKkIJAAHfCYSxXLKxykTaXklFnTMm16MkHbTbsUc3xsJ7BGu8wKUnQWZyRcySTw6sDkhVOWqzrMlZZtl9pc3eJpoNBY5EGEfAh0EXAB3aCemACIfFntvnxvgAmAAMoOBgOg3wAMoJJrAFBg4LgMGAQCA5MAod8ACoUYQUNE4gBSYC2CG-okMQA&debug=false&forceAllTransforms=false&shippedProposals=false&circleciRepo=&evaluate=true&fileSize=true&timeTravel=false&sourceType=module&lineWrap=true&presets=env&prettier=false&targets=&version=7.4.5&externalPlugins=).

If you don’t want to avoid these features altogether, you have a couple ways of reducing their impact:

1. If you’re authoring a library, consider using [@babel/runtime](https://babeljs.io/docs/en/babel-runtime) in concert with [@babel/plugin-transform-runtime](https://babeljs.io/docs/en/babel-plugin-transform-runtime) to deduplicate the helper functions Babel puts into your code.
2. For polyfilled features in apps, you can include them selectively with [@babel/polyfill](https://babeljs.io/docs/en/babel-polyfill) via [@babel/preset-env’s useBuiltIns: “usage”](https://babeljs.io/docs/en/babel-preset-env#usebuiltins) option.

This is solely my opinion, but I believe the best choice is to avoid transpilation altogether in bundles generated for modern browsers. That’s not always possible, especially if you use [JSX](https://reactjs.org/docs/introducing-jsx.html), which must be transformed for *all* browsers, or if you’re using bleeding edge language features that aren’t widely supported. In the latter case, it might be worth asking if those features are really necessary to deliver a good user experience (they rarely are). If you arrive at the conclusion that Babel must be a part of your toolchain, then it’s worth peeking under the hood from time to time to catch suboptimal stuff Babel might be doing that you can improve on.

## Improvement is not a race

As you massage your temples wondering when this horrid JavaScript hangover is going to lift, understand that it’s precisely when we rush to get something out there as fast as we possibly can that the user experience can suffer. As the web development community obsesses on iterating faster in the name of competition, it’s worth your time to [*slow down a little bit*](https://en.wikipedia.org/wiki/Thinking,_Fast_and_Slow). You’ll find that by doing so, you may not be iterating as fast as your competitors, but *your product* will be *faster* than theirs.

As you take these suggestions and apply them to your codebase, know that progress doesn’t spontaneously happen overnight. Web development is a job. The truly impactful work is done when we’re thoughtful and dedicated to the craft for the long haul. Focus on steady improvements. Measure, test, repeat, and your site’s user experience will improve, and you’ll get faster bit by bit over time.

*Special thanks to* [*Jason Miller*](https://twitter.com/_developit) *for tech editing this piece. Jason is the creator and one of the many maintainers of* [*Preact*](https://preactjs.com/)*, a vastly smaller alternative to React with the same API. If you use Preact,* [*please consider supporting Preact through Open Collective*](https://opencollective.com/preact)*.*
