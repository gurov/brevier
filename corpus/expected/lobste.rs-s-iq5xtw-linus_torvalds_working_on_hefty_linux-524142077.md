# [Linus Torvalds Working On A Hefty Linux Kernel Improvement With Claude Code](https://www.phoronix.com/news/Linus-Torvalds-Claude-Code)

*phoronix.com · linux, vibecoding · 61 votes · oceanhaiyang · 12 October 2026*

## 101 comments

<a id="c_pdxjuf"></a>**bmo** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_pdxjuf)

Reading the [patch message](https://git.kernel.org/pub/scm/linux/kernel/git/torvalds/linux.git/commit/?h=noref-filemap-read-with-llm&id=4c0f3697118eb3a708ea79175f7f07c43339bfab) is honestly disheartening. I think everyone can agree that AI has some utility as a code reviewer at least. Whether to use it to generate code is controversial but certainly many people decide to. But replacing actual written communication, in English, for humans, with slop is a line that I wish we could all draw. According to Linus "the first draft" was written with an LLM.

> No reference, no on-stack bounce buffer, no seqcount, no check after the copy, and no limit on the size of the read.

> They stopped at what the validation cannot see:

> Known problems, and what I'd like eyes on

I'm so viscerally sick of this style of writing that communicates so poorly and shows lack of human thought behind it.

> <a id="c_m8x1wb"></a>**simonw** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_m8x1wb)
>
> Yeah, that prose is gross.

> > <a id="c_vtkc0g"></a>**dlisboa** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_vtkc0g)
> >
> > I really think it’s a native language blindness a lot of people here have, but I’ve been reading technical “programming English” for 20+ years and LLM speak is exactly like tech blog speak from 20 years ago. As a non-native speaker with a flair for language acquisition I see this a mile away.
> >
> > LLMs just copied human production, people have been writing this prose style forever and it has been ingested as training data.
> >
> > Like I just went to your blog Simon and I can go to the earliest entries and read a style that LLMs would mimic today. The seeds of the mimicry are there, maybe not all in one article but as an aggregate.
> >
> > And I mean this as no disrespect whatsoever, it’s just language and there’s nothing necessarily wrong with it. They copied you, not the other way around.

> > > <a id="c_lvdgwc"></a>**andyc** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_lvdgwc)
> > >
> > > Why are you so confident of that, if you're a non-native English speaker? I'm a native English speaker, and I have also written and read many technical blog posts for over a decade, and I see Claudish all over the place.
> > >
> > > If what you said were true, then I would have never have seen Claudish, and that's obviously not true.
> > >
> > > If you genuinely haven't seen Claudish in English on the Internet in the last year, then I would suggest that's because your English is not as native as you think it is. Or maybe you just don't use Claude that much. (I use Claude all the time, because it is useful, but that also makes its tics very apparent when I encounter them on a blog. And yes it immediately makes me suspicious of sloppy thought.)
> > >
> > > * * *
> > >
> > > Actually one prominent and sorta sad example is that I'm basically certain Professor Joe Hellerstein has been using Claude to write blog posts since April 2025. The timing is pretty suspicious on its own.
> > >
> > > [https://jhellerstein.github.io/blog/](https://jhellerstein.github.io/blog/)
> > >
> > > e.g. [https://jhellerstein.github.io/blog/cost-of-committing/](https://jhellerstein.github.io/blog/cost-of-committing/) - so many Claudish tics here
> > >
> > > I was reading his papers on distributed systems (CALM, Bloom, etc.) over 10 years ago. That was technical prose.
> > >
> > > I don't know if he had a blog prior to 2025, but I bet if he did, you can compare the prose and see that it's now done with Claude.

> > > <a id="c_bunlfc"></a>**altano** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_bunlfc)
> > >
> > > If that were true all llms would sound exactly like each other and we wouldn’t be able to distinguish Opus 5.x prose from natural English prose in one sentence, but alas…

> > > <a id="c_9tkzyb"></a>**dlisboa** · replying to altano · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_9tkzyb)
> > >
> > > I’d bet my house I could give you 10 paragraphs and you can’t reliably get the model right for any of them or distinguish from an actual English author.

> > > <a id="c_ziq18j"></a>**mandeep** · replying to dlisboa · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_ziq18j)
> > >
> > > Personally, I can definitely distinguish Claude prose from other models' prose. It has a very distinctive writing style. Between other models, I generally cannot tell no.

> > > <a id="c_cmbcwc"></a>**viraptor** · replying to dlisboa · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_cmbcwc)
> > >
> > > It's possible to do automatically - we've already got services that do it reliably. For human recognition, I wouldn't risk your house. There are specific phrases that are common to family models. Claude has the load bearing, seams and list of no-s for example.

> > > <a id="c_9d2pfw"></a>**gjvc** · replying to altano · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_9d2pfw)
> > >
> > > > If that were true all llms would sound exactly like each other and we wouldn’t be able to distinguish Opus 5.x prose from natural English prose in one sentence, but alas…
> > >
> > > I was able to distinguish between Opus 5.x prose and Opus 4.6 (which is very much my favourite), so there's that new skill I have :-)
> > >
> > > pssssst, "distinguish between" not "distinguish from", in the same way it's "different from" and "similar to" :-)

> > > <a id="c_gftqrt"></a>**altano** · replying to gjvc · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_gftqrt)
> > >
> > > Both constructions are valid: [https://www.oxfordlearnersdictionaries.com/definition/english/distinguish](https://www.oxfordlearnersdictionaries.com/definition/english/distinguish)

> > <a id="c_banhwu"></a>**signal-11** · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_banhwu)
> >
> > [simple-technical-english](https://github.com/AminBlg/SimpleEnglish) helps somewhat.

> <a id="c_m30bxt"></a>**meline** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_m30bxt)
>
> I wish there was a way to better classify this awful style of pose that all LLMs generate. I'm actually writing a paper at the moment on the topic of AI slop. While I've had incredibly surprising success with regexes and even a simple naive Bayes classifier, as a human I can smell this style of prose almost instantly, yet it seems hard to teach a computer (in an explainable way at least).
>
> It's everywhere now though. I feel like I'm going insane, but my partner and I cross-check each other and it truly is just everywhere.

> > <a id="c_mv2idw"></a>**rbr** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_mv2idw)
> >
> > I once watched a presentation on a piece of research that shows LLMs change the meaning of a text ever so slightly when you feed text through it (because of synonyms, adding/removing repetitions, etc.). The particular graph I remember is some kind of point cloud, where two clusters can be seen. One of human written text, and another of their LLM "improved version". The paper showed that an LLM tends to push writing in a certain direction, if you project each before and after piece of text on some kind of 2D plane.
> >
> > Based on the position in that field, you could probably give an estimate about LLM taintedness... I'll have a look if I can find the paper but it's been a while.
> >
> > Edit: gotcha: [https://arxiv.org/html/2603.18161v1](https://arxiv.org/html/2603.18161v1)

> > > <a id="c_hdxyaj"></a>**meline** · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_hdxyaj)
> > >
> > > Ah that is absolutely wonderful! Thank you for that reference, I hadn't come across that yet in my review, but that sounds really useful for my own work. I had actually tried to do a PCA on the TF-IDF vector embeddings of my "human written" and "AI slop" dataset, but didn't get very far with that. The people at Pangram seemed to do much better: [https://www.pangram.com/pangram-space](https://www.pangram.com/pangram-space). I suspect that in that arXiv paper cluster analysis, there would be a distinct grouping between "LLM improved" and "AI slop", which would be very interesting to look into.
> > >
> > > I must say as someone new here it is wonderful how helpful lobsters folks are :)

> > <a id="c_e9gehx"></a>**bmo** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_e9gehx)
> >
> > [Wikipedia](https://en.wikipedia.org/wiki/Wikipedia:Signs_of_AI_writing) has done a pretty good job listing many the common tells but you're right that it's much more than that. It's a fundamental alienness, a sense that "no human would try to communicate that way." There are mundane words like "sits" or "gap" that while not incorrect are suddenly much more frequent and used in tortuous ways where there are obvious better alternatives - like "knowing where each responsibility sits" (and ugh, just google that phrase and see if you can find any human writing in the results.)

> > > <a id="c_mtr23n"></a>**meline** · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_mtr23n)
> > >
> > > That Wikipedia link is wonderful and something I've actually referenced in my work already! What you say about tortured phrases is very interesting as well, I remember reading this work: [http://arxiv.org/abs/2107.06751](http://arxiv.org/abs/2107.06751) although I'm not sure how relevant it is to modern LLMs. It *is* really useful to basically grep for certain "tortured phrases" that exclusively AI writes, you know, the "load-bearing" classic and similar as you mention. But it is really that awful copywriting, corporate-speak, alien style of speaking which is hard to explain in words but you just *know* it, and that's hard to teach the computer.

> > > <a id="c_cwwx9m"></a>**k749gtnc9l3w** · replying to meline · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_cwwx9m)
> > >
> > > > > There are mundane words like "sits" or "gap" that while not incorrect are suddenly much more frequent and used in tortuous ways where there are obvious better alternatives
> > >
> > > Sounds like an explanation of language proficiency difference B1/B2 vs C1, honestly
> > >
> > > > It is really useful to basically grep for certain "tortured phrases" that exclusively AI writes, you know, the "load-bearing" classic and similar as you mention.
> > >
> > > > alien style of speaking
> > >
> > > My native way of speaking checks all these boxes, except that I have my own set of verbal tics that are not cloned all over the internet in the amounts that make them universally recognisable.
> > >
> > > Stylometry has always been a thing, and monocultures have always been problematic. Even if Claude wrote *perfectly*, the fact that a single entity writes 10% of internet would make its exact style unbearable for many.
> > >
> > > I'd rank human-rambling < LLM-rambling < dense-LLM-writing < dense-human-writing < so-dense-it-is-called-bad-human-writing. LLM rambling has this weird uniformity that makes it more skimmable than human-written low-density texts.

> > <a id="c_oearzz"></a>**altano** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_oearzz)
> >
> > Bryan Cantrill wrote [an article](https://bcantrill.dtrace.org/2026/09/05/the-revolt-of-the-reader/) saying Pangram's AI detector is really good at this:
> >
> > > A little over a month ago, they introduced Pangram 4, which I found to be a step-function improvement over the already-impressive Pangram 3: in my experience it has an astonishingly low false positive rate and low false negative rate (which is to say: very high accuracy!). I am finding it to be so effective (and the loss of trust in voices that use LLMs to be so precipitous) that I recently extended RFD 576 to be explicit about public writing, specifically mandating that public Oxide writing be reported by Pangram as human-authored.

> > > <a id="c_0qsrl2"></a>**meline** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_0qsrl2)
> > >
> > > Indeed, Pangram is really good in my experience too, especially insofar as keeping false positives extremely low. The main problem is that I'm screening an extremely large number of texts in my work (>100,000) so just the time and cost of running each file past Pangram is prohibitive, I expect. If I can ever get funding for this research though, that would be my immediate follow-up, for sure.

> <a id="c_bkzuin"></a>**junot** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_bkzuin)
>
> Anubis is giving my browser a hard challenge on my phone with low battery... to read a slop patch message. I aborted it after a few seconds.
>
> I guess this list is for machines only, no humans?

> > <a id="c_kmwh7c"></a>**avh-on1** · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_kmwh7c)
> >
> > Difficulty 5 for me. Took about 15 seconds for my desktop to load what amounts to a few-kilobyte pure-text email. Like dial-up.
> >
> > (Last night, I got a page with anubis difficulty 20. Never seen that before. Took over a minute...)

> <a id="c_z6n6mb"></a>**lumi** · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_z6n6mb)
>
> We should draw the line at any GenAI use at all. It's unethical and anti-social tech. We should not accept it.

> <a id="c_398stw"></a>**viraptor** · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_398stw)
>
> This is just a branch found in the wild, right? It's not actually submitted and meant for reading yet?
>
> I'd hold the judgement on the message which may be just auto-generated without paying much attention, because it's not going to be posted as-is.

> <a id="c_ca9w5s"></a>**dbs** · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_ca9w5s)
>
> Meanwhile git.kernel.org deploys Anubis in front of itself

> > <a id="c_v8oecm"></a>**avh-on1** · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_v8oecm)
> >
> > They've written about that before; that a significant percentage of the global CPU pool was being spent rendering old commits and diffs in git-web, being requested by scraper bots.

> <a id="c_lidcxl"></a>**cesarandreu** · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_lidcxl)
>
> I keep thinking that socially we are still developing antibodies to such writing, and people develop resistance at varying rates. I don't know how much exposure Linus has gotten to AI generated prose, but I hope if there's reasonable pushback he'll adjust.
>
> One comparison I would make is when image generation trends started going viral, the first few images were kinda cute and fun to play around, but once you notice their use everywhere it loses its appeal very quickly. Although I've noticed that many older generation people are increasingly getting one-shot by AI models.

> <a id="c_qilqeq"></a>**Papageek** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_qilqeq)
>
> Using AI to generate code is controversial in the same way flat earthers say round earth is controversial.

> > <a id="c_rn7lri"></a>**bmo** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_rn7lri)
> >
> > It's objectively controversial, people have sharp disagreements about not only whether but how to use LLMs and there's a wide range of opinions. But also, read the room. "Half the commenters here are flat earthers" is not really conducive to discussion.

> > <a id="c_hsmhsr"></a>**sunshowers** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_hsmhsr)
> >
> > Using AI to generate prose is significantly less controversial, in that it is widely disdained.

> > > <a id="c_yx4z56"></a>**Halkcyon** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_yx4z56)
> > >
> > > I find it more controversial there since the point of prose is *communication*.

<a id="c_t30reu"></a>**simonw** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_t30reu)

> I personally am not happy about this, the last place I want AI to touch is my Linux.

Is that for ethical reasons or code quality reasons or both?

If it's the former then fair enough. If it's the latter, one could make a credible case that Linus Torvalds is the most experienced code reviewer in the world.

> <a id="c_trweh4"></a>**emk** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_trweh4)
>
> Yeah. If I had to pick literally one single human to read through 100,000 lines of bullshit PRs and firmly reject the bad ones, well, Linus has spent the majority of his "development" time doing that for 30 years now. And he's not especially shy about rejecting bad code. (By which I mean mainstream media was starting to write about his rejections, and so he was sent for remedial social skills training.)
>
> So just to troll a bit, my new position on this issue is that *only* Linus should be allowed to use LLMs. I trust Linus to read the code and tell Claude what he really thinks.

> > <a id="c_r5ysgx"></a>**moltonel** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_r5ysgx)
> >
> > -- This is utter garbage, whatever inferred that moronic code should get its weights checked.
> >
> > -- You're absolutely right !

> > > <a id="c_mkwm6c"></a>**skade** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_mkwm6c)
> > >
> > > Didn’t Anthropic just ban that?

> > <a id="c_l36ssc"></a>**k749gtnc9l3w** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_l36ssc)
> >
> > And the code he has to review is not even a product of human decision making half of the time, it is often a product of corporate politics, and corporate politics is neither less alien nor more aligned with humans than LLMs!

> <a id="c_a3tuu6"></a>**jackdaniel** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_a3tuu6)
>
> I can't comment that he will fail to review LLM code, but code churned by LLMs has different failure modes than human written code, so reviewing it is different. Notably LLMs easily use good terms and the code looks plausible, disregarding whether it is correct or wrong; while sloppy (no pun intended) code made by a person has often tells -- i.e noticeable habits, wrong terms pointing out that they are new to the domain etc.

> > <a id="c_pdorlm"></a>**emk** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_pdorlm)
> >
> > Interesting. I review (and reject) a lot of LLM code, much of it in Rust or other typed languages. Less than 10% of the problems I have found are caused by subtle bugs. At least 90% of problems could be described as "over-expedient architecture" or "badly factored architecture." This may be because I use a long list of linters and other automated checkers, and I'm not shy about asking LLMs to write proptests.
> >
> > Honestly, I like to imagine I'm careful. But the LLM probably finds as least as many of my bugs as I find of its. The biggest complaints I have all lie higher up.
> >
> > (People who don't read the code, who don't design good test strategies, or who use agents swarms mostly get what they deserve.)

> > > <a id="c_logydi"></a>**jackdaniel** · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_logydi)
> > >
> > > sure; my point is that reviewing llm code and human code is not the same thing, even when we put aside the sheer volume and velocity - so the masterclass review skills of Torvalds of human coda are not necessarily as good guardrail as the grandparent suggests.
> > >
> > > edit: to add a metaphore - a top class swimmer is not necessarily a top class runner and vice versa

> > <a id="c_btm3mk"></a>**k749gtnc9l3w** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_btm3mk)
> >
> > I think Linus Torvalds has rejected quite a few patches where a competent person implemented under protest the outcome of a corporate decision-making process. Which might not have low-level tells but might be absurd globally.
> >
> > Corporations are often human-hostile alien-logic decision-making machines, just like LLMs. They are just slower.

> > > <a id="c_bmrlpa"></a>**jackdaniel** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_bmrlpa)
> > >
> > > procedurally these may be similar, but my point is that LLMs get to the goal differently than humans, so the review skills are not obviously transferable between different types of authors. corpo logic may be alien, but that applies to outcomes, not means of coding.

> > > <a id="c_nrustp"></a>**k749gtnc9l3w** · replying to jackdaniel · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_nrustp)
> > >
> > > «You ship your orgchart» definitely includes corporate logic leaking into the code. And for drivers, lies a corporation tells about their product sometimes also become constraints on the code.

> <a id="c_yzc9x4"></a>**dvogel** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_yzc9x4)
>
> This is true but I find it almost entirely irrelevant. Everyone's code review practices are skewed by their incentives. Therefore "code review" is not one thing and very little of it is objective.
>
> When accepting the code from a human-submitted patch set, Linus loses none of his own productivity when rejecting the code. One of his super powers is having almost no specific productivity goals for the kernel code. He has plenty of productivity goals related to the kernel development process and he would certainly see a change-less release cycle as unproductive. However there has been no lack of supply on the change side for 30 years. When he is reviewing the code of an LLM, in the pursuit of some code production goal of his own, the quality bar for him accepting the code is lower. How much lower is an open question. However I think it is beyond debate that the demands of code review for others is drastically different than code review for our own ends.
>
> Critically, rejecting a human-submitted patch has a long term productivity benefit in many cases. Each of those rejections is a chance for Linus to not just make the engineer better informed about the kernel but also more familiar with the processes and values of the kernel project. I think it is safe to say that no one has become a subsystem maintainer without having many patches rejected. When Linus rejects a code generation from an LLM there is barely even a signal of positive intent to discard it. The code just goes unused.

<a id="c_wiifqy"></a>**pmbauer** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_wiifqy)

Linus is going to test the limits of Anthropic's deranged policy against psychologically abusing claude, and I'm here for it.

<a id="c_ujyha2"></a>**2** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_ujyha2)

We have to accept that software engineering has changed a lot, and what we used to love doesn't exist anymore.

Overall there is a lot more noise, open-source is a mess, we learn about many data leaks every day, we get spammed about new business concepts that are more than ideas but half baked apps, we lose our jobs, but the software quality of some projects is improving.

> <a id="c_zpckbt"></a>**csomar** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_zpckbt)
>
> > but the software quality of some projects is improving
>
> I'd be very interested to know *which* software project quality has improved over the last year. Almost every software I am using had dropped to insane low levels. I am using an M5 with the latest macOS and the amount of glitches/bugs/defects is beyond shameful.

> > <a id="c_quavw0"></a>**qznc** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_quavw0)
> >
> > I'd say code at my $WORK looks cleaner and more consistent now. It also tends to handle more edge cases and is often more backwards-compatible. Some quality warnings (e.g. too many line in this function) improved because LLMs can refactor that easily.
> >
> > The downside is that it is a lot more code. More redundancies. Implementing stuff nobody ever needs. Testing stuff nobody cares about. We had a four eyes principle but in some cases it is zero eyes now.
> >
> > In terms of end user experience, I'm not sure either way.
> >
> > I guess it depends on how exactly you define quality.

> > > <a id="c_dkv3o4"></a>**elliotmorris** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_dkv3o4)
> > >
> > > The anecdotal-ness of it all continues to bug me. I'm willing to accept that we won't see actual end user or economically observable benefits for a while, but I have to wonder how long that is.
> > >
> > > Perhaps it's just me but, "It is obviously an improvement" is not satisfying. It isn't obvious to me, and there are reams of "obviously" better techniques from the past which are now looked on with a more critical eye. (OO, Agile, Microservices).
> > >
> > > Whether it's satisfying to me personally or not though isn't really here nor there, alas.

> > > <a id="c_iepiqu"></a>**csomar** · replying to elliotmorris · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_iepiqu)
> > >
> > > This is roughly the same god argument I was fed at high school. Basically, it's all strong and powerful and everything but just take our word for it and someone did see the prophet like a thousand years ago.
> > >
> > > No one has answered my question and it's a simple one: is there an LLM made software that is not just a bunch of garbage put on top of each other? The answer is that there is simply *not*. If that was possible, *all* the tech stack will be replaced in less than a day. LLMs are really really fast in writing code. You can write a new browser (size of tokens, I mean) on a $100 subscription.

> > > <a id="c_bckvo6"></a>**danlamanna** · replying to csomar · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_bckvo6)
> > >
> > > > No one has answered my question and it's a simple one: is there an LLM made software that is not just a bunch of garbage put on top of each other?
> > >
> > > Is it a simple one? What does LLM made mean? What do you mean by software? Surely something like ghostty is a good example of heavily LLM assisted software. There's been highly successful LLM ports of libraries too (e.g. chardet).

> > > <a id="c_awhssh"></a>**elliotmorris** · replying to csomar · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_awhssh)
> > >
> > > I'd hesitate to make this demand for examples so starkly.
> > >
> > > I agree with you, it's shocking and damning that there isn't a clear example yet given the investment in the space, and it's not like we haven't had "game-changing" LLM's for years now. However, there probably will eventually be an example of a decent piece of heavily LLM generated software that is in a serious, non AI domain, isn't just a personal project with a few dozen users, and is long-term profitable even considering subsidies. It'll occur by happenstance if nothing else just due to the amount of attention on things.
> > >
> > > A couple of anecdotal examples are not proof though, it's the economic (and ethical!) justifications that matter, and if we go around demanding a single example eventually we'll get one and it'll leave us in a spot where it's difficult to speak about the real macro problems.

> > > <a id="c_atfanw"></a>**kubanczyk** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_atfanw)
> > >
> > > I broadly agree, code at $WORK really has less hard bugs and ad-hoc quirks. But it's responsible use of a flamethrower. It only goes so far.
> > >
> > > People will start skipping reviews, these extra layers will explode. The models aren't good enough for "zero eyes" now.

> > > <a id="c_d8dri0"></a>**gonz** · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_d8dri0)
> > >
> > > > Some quality warnings (e.g. too many line in this function) improved because LLMs can refactor that easily.
> > >
> > > Together with nonsense like this it actually seems like your code base got worse; you're covering edge cases that likely aren't even possible (I say this from experience using LLMs; they're very bad at considering whether edge cases can actually happen, and/or will write code as if they always can, which is not useful), you're applying dubious rules like "too many lines in a function" that absolutely will make code worse and harder to read over time, you've got features you don't need or want on the pile of shit to maintain.

> > <a id="c_yekz0f"></a>**sunshowers** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_yekz0f)
> >
> > macOS has always been built to a not very high standard.
> >
> > I think the work and personal projects I work on have gotten notably better, often with LLM use. A lot of the work stuff is probably a bit harder to communicate, but with nextest I was able to add really cool instrumentation with USDT probes, run recording/replay/rerun failing tests, and Chrome trace/Perfetto exports.

> > <a id="c_0imupe"></a>**mxey** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_0imupe)
> >
> > I haven’t updated yet but I thought the consensus was that macOS 27 is better than 26?

> > > <a id="c_lvo5sa"></a>**binjip978** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_lvo5sa)
> > >
> > > I think it worse, macOS 27 steals 25GB from your disk on unremovable slop that you can't delete without some magic [https://github.com/4evy/pared](https://github.com/4evy/pared). Yeah it's improve UI a bit, but it's hardly an achievement given how terrible liquid glass was in macOS 26.
> > >
> > > Also I think they deleted rosseta, so amd64 apps no longer working, but you can install it back, they plan completely remove in next release.

> > > <a id="c_1zhepu"></a>**snazz** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_1zhepu)
> > >
> > > It fixes a number of bugs and longstanding performance issues, and added some new ones. Most of the UI design issues in Tahoe that were almost universally panned (varying and huge corner radii, sidebars on top of content, icons in menus) were rolled back.

> > <a id="c_zso9fc"></a>**2** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_zso9fc)
> >
> > Linux and Firefox for example. My side projects too.

> <a id="c_blrfmf"></a>**sardaukar** · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_blrfmf)
>
> A little part of me thinks that the open-source thing is by design. Maybe I'm giving the Machine God worshippers in the Valley too much agency, but it seems like making life hell for FLOSS and the funding going into Omarchy to win the hearts and minds of coders before "socialism" and Libre ideas corrupt them is an intended outcome.

<a id="c_mqyfey"></a>**conartist6** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_mqyfey)

Oh how the mighty have fallen.

But that's what AI does, it brings mighty people low; makes them common.

> <a id="c_xqi8sv"></a>**pushcx** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_xqi8sv)
>
> Yesterday I got an email responsibly disclosing two vulnerabilities in Lobsters, found with LLMs. They've been there 8 years. I have lost track of how many years-old vulns and bugs that LLMs have been identified in our codebase in the last two years. LLMs are making [Linus's Law](https://en.wikipedia.org/wiki/Linus%27s_law) true. Feels very much like the opposite of common, or that common is being redefined to a higher baseline quality level.

> > <a id="c_lfevvp"></a>**conartist6** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_lfevvp)
> >
> > I see LLMs as scab, as scar tissue. They don't solve anything, but they rush in to fill a void.
> >
> > Software has shown itself to be one such void: there's massive unmet demand for software literacy: both to find bugs in existing software, and to create new software or customize existing tools.
> >
> > I just want to make sure the vision for the future is one of broad human code literacy. Nothing about AI should change the imperative that we break down the systemic barriers that prevent more people from being literate. Only broad human literacy will result in the creation of systems that sustainably and continuously serve the needs of their users.

> > <a id="c_hghw3l"></a>**csomar** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_hghw3l)
> >
> > That doesn't mean they are good/safe to write code with. I'd harden a certain code/infra with by throwing LLM chaos monkeys at it but that's the limit of that.
> >
> > ps: I'm boycotting/stopping using LLMs to write code but it's merely a personal preference at this point rather than ideological (I was happy to jump at them at first but after a year now, their ROI is negative. I'd be happy to try new models and I do that). That being said, for discovery/hacking, they are tireless and can go through a tons of garbage very fast.

> <a id="c_wfd2vo"></a>**oceanhaiyang** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_wfd2vo)
>
> \[Comment removed by author\]

> <a id="c_e565vo"></a>**oceanhaiyang** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_e565vo)
>
> > But that's what AI does, it brings mighty people low; makes them common.
>
> That is quite profound.

> <a id="c_nw7hjo"></a>**pert** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_nw7hjo)
>
> I’d be interested to know what “common” means here? If you are against this for ethical reasons then I understand, but what does “common” mean in this context?

> > <a id="c_izxxfq"></a>**conartist6** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_izxxfq)
> >
> > Once you start spending your time talking to AI, you start to lose what makes you unique. You stop generating uniqueness and start consuming sameness. People are natural mimics and empaths. You can't spend your time conversing with a statistical model and not be cognitively affected by it.

> > > <a id="c_o0uqdi"></a>**Student** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_o0uqdi)
> > >
> > > Yes but consider the effect on people who are below average.

> > > <a id="c_slwzd9"></a>**bilinguliar** · replying to Student · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_slwzd9)
> > >
> > > Misfits make history. The majority of the population takes the fruit of their work for granted and, as Bill Burr joked, complains about the slow Wi-Fi on the plane.

<a id="c_iys7t8"></a>**pkubaj** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_iys7t8)

Linux already contains LLM-generated code anyway. And you know what? It doesn't matter because there's no way to avoid LLM-generated code. GCC and LLVM can be used for Linux compilation and they both contain LLM-generated code already.

Looking at [https://codeberg.org/ethical-foss/open-slopware#operating-systems](https://codeberg.org/ethical-foss/open-slopware#operating-systems), to be LLM-free, you would have to use an OS that is against LLM, compiled by a compiler written without LLM. That removes all the Linux distributions, BSD systems (even NetBSD, against using LLM, is using GCC and LLVM) and even Illumos. Probably the only one left would be 9front (Redox uses Rust compiler, which also was written with LLM help). Good luck using it for everything.

Also, keep in mind that firmware code is not granted by the heavens, but written - or these days probably "hallucinated" is a better word. As it turns out: [https://codeberg.org/ethical-foss/open-slopware#bootloaders](https://codeberg.org/ethical-foss/open-slopware#bootloaders) even coreboot allows for LLM, U-Boot as well.

> <a id="c_zja7ny"></a>**tuxes** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_zja7ny)
>
> Some people are upset about prevalent LLM usage. "It doesn't matter because there's no way to avoid LLM-generated code." feels a bit dismissive/inconsiderate?

> > <a id="c_b2r79c"></a>**adrien** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_b2r79c)
> >
> > I'm against LLM usage by default (very roughly: before showing me something from one, prove me first it's not bad). I think there's a lot of slop and I fear for the quality of software that I use in the not-so-distant future. I hate being fed AI slop online or outside (wtf: large 3\*4 meters ads in the streets don't even make sense anymore).
> >
> > But.
> >
> > Yes, it's unavoidable. Not because AI will rule the world by virtue or by force. Simply because a large number of people are using it without talking about it.
> >
> > Just an hour ago, I was looking at a bug report that was definitely AI-authored; it had the typical "why this is a real bug and why it matters" but no mention of an LLM.
> >
> > It makes no sense to try to fully avoid it and declare projects dead and untouchable once anything LLM gets near it.
> >
> > It's basically like trying to avoid anything that touches fossil fuels or increases global warming, or anything related to warmongers, slavery, crimes, ... . You can only minimize the exposure, take a stand, do your best. I really want to avoid that shit too but purity tests don't work in any field.
> >
> > PS: I came across an article a few days ago that mentionned that global warming had started before the industrial revolution and was linked to agriculture, i.e. food production. It wasn't denying anything about fossil fuels and was instead criticizing compensation mechanisms through plantations as they cannot work as well as calculated. Was interesting. And yeah, if you want to avoid causing any global warming, you have a lot of work ahead for your food.

> > > <a id="c_nsq9sj"></a>**elliotmorris** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_nsq9sj)
> > >
> > > I question how we are to continue our existence on this planet if something comparable to climate change can just appear over the course of half a decade, and go from zero -> it is everywhere and there is very little you can or should do about it. We were reducing our dependence on fossil fuels, not increasing it, we shouldn't add more categories of thing that follow these patterns.
> > >
> > > Is our ambition so low? Is our will to survive so weak that we simply must accept any change on this scale. May we not even rage against the dying of the light?

> > > <a id="c_kx0ytr"></a>**adrien** · replying to elliotmorris · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_kx0ytr)
> > >
> > > I'm not sure if I was clear: I'm not advocating doing less. I'm advocating against purity tests that are doomed to fail. It sucks having to repair or undo; it's much better to prevent and limit.
> > >
> > > The cards we're being dealt with cannot be changed though. The problem is that a number of people are coming up with technical solutions while the problem isn't technical but social/politics.
> > >
> > > Also, what do you do with large\_project\_foo that has LLM-generated-and-not-ethical code among human-authored changes but decides in a year to reject everything LLM. Revert generated patches? Rewrite them? Go back to an earlier commit? First, that's not going to work; and second, what would be the benefits and goals? You won't undo the bad stuff. You must also pay attention to the costs to humans that are associated with such moves.
> > >
> > > And finally, lots of this feels like pointing fingers at various projects and individuals but not at corporations, tech bros, politics. All while locking themselves in an ever-shrinking place by definition.

> > > <a id="c_ojnw6v"></a>**elliotmorris** · replying to adrien · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_ojnw6v)
> > >
> > > Aye, I get you, and it's a fair point. I don't think much of the discourse, including in this thread really, is a real call to praxis around avoiding all LLM artifacts, because as you say there's no real feasible route to do that anymore, although I wish there were and am angry about it.
> > >
> > > I suspect folks are, much like myself, trying to fence out an area where they are allowed to publicly feel emotions on this, to not like it and not have to constantly justify why. The whole "it's inevitable" thing pokes at that feeling

> > > <a id="c_r2ermn"></a>**adrien** · replying to elliotmorris · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_r2ermn)
> > >
> > > Yep, same feelings.
> > >
> > > I think I'll start saying that "it has not been avoided" as it is not a judgement about future or a prediction but an observation about the present.

> > > <a id="c_37wfk6"></a>**trintron** · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_37wfk6)
> > >
> > > Would you mind sharing that article about agriculture as a possible starting point for global warming?

> <a id="c_s75ld5"></a>**lumi** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_s75ld5)
>
> Just because it's prevalent doesn't mean we should give up. The resistance is growing and will keep growing. It's still quite early days for the resistance and we are gaining ground.

> > <a id="c_j36eps"></a>**abnercoimbre** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_j36eps)
> >
> > Hear, hear!

> <a id="c_41slqr"></a>**tombh** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_41slqr)
>
> > It doesn't matter because there's no way to avoid LLM-generated code
>
> There's no way to avoid a lot of things, but that doesn't justify them.

> <a id="c_wwhpxu"></a>**bsder** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_wwhpxu)
>
> > It doesn't matter because there's no way to avoid LLM-generated code.
>
> You DO realize that we have yet to actually even settle the LEGALITY of all the LLM training, right?
>
> These companies are trying very hard to make that fait accompli before the legal system catches up with them. They're following the Amazon and YouTube and Uber playbooks only on fast-forward.

> > <a id="c_hbmakd"></a>**gulbanana** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_hbmakd)
> >
> > i hope they fail for once, but it has worked so far

> <a id="c_wg6tju"></a>**moltonel** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_wg6tju)
>
> Saying "you're not avoiding LLMs, because your compiler has some LLM-generated code in it" seems as useful as saying "no point in preferring $LANGUANGE because it's all assembly in the end".
>
> We don't control the whole stack, but we can still have strong opinions and push in that direction. Tech is full of compromises, no decision is absolute.

> > <a id="c_7wmnww"></a>**pkubaj** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_7wmnww)
> >
> > Frankly, I get the reason why might want to prefer to write software in a language he likes. But I don't get why someone would choose to run specific software because it's written in a specific language (as is the case with many of Rust rewrites of popular software, where the primary feature is that it's written in Rust). Same with LLM - I don't care whether some software was written with LLM or not, I care about its performance, stability, features and license.

> > > <a id="c_wmqtny"></a>**k749gtnc9l3w** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_wmqtny)
> > >
> > > The language a tool is written in might be (or sometimes not) an indicator of:
> > >
> > > - ease of integration with some other tool (based on whether there is a language match, good FFI, or impedance mismatch)
> > > - distribution of bugs (Rust code with few unsafe blocks is very likely to have fewer arbitrary code execution bugs than C, although some can be just converted to panics, but that's still better)
> > > - expected ratio of breaking changes to bug fixes; especially when the language ecosystem forces those breaking changes all the time on its own
> > >
> > > Other traits of how a tool was written could also be useful signal of the same and similar properties.

> > > <a id="c_kfghui"></a>**pkubaj** · replying to k749gtnc9l3w · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_kfghui)
> > >
> > > > ease of integration with some other tool (based on whether there is a language match, good FFI, or impedance mismatch)
> > >
> > > That's something I would take as a feature.
> > >
> > > > distribution of bugs (Rust code with few unsafe blocks is very likely to have fewer arbitrary code execution bugs than C, although some can be just converted to panics, but that's still better)
> > >
> > > And that one is stability. And I'm really curious how much more stable new rewrites are than the code written for (often) over 20 years. I think there were some news about issues with Ubuntu with their new coreutils rewrite.

> > > <a id="c_vohsm9"></a>**k749gtnc9l3w** · replying to pkubaj · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_vohsm9)
> > >
> > > Distribution of bugs is different from stability. It can be less stable, less compatible, have more bugs, and still because they are different kind of bugs the bugs could be less of a burden operationally.
> > >
> > > In the extreme case, imagine that you have one `less` version that panics if anything is not perfectly valid UTF8 plain text (but it only ever panics) despite the documentation saying it sanitises the input no matter what; and you have another `less` version that is much more stable overall but there are five kinds of corrupted escape sequences (that do not occur on their own in the wild) in the files that make it execute the rest of the file as a shell script. The latter is more stable and has fewer bugs. Still, if I am given an alleged `.md` by an untrusted party, the former `less` might be preferable.

> <a id="c_tqnn4i"></a>**derp\_alert** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_tqnn4i)
>
> Is OpenBSD using LLM generated code?

> > <a id="c_vvk1xt"></a>**pkubaj** · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_vvk1xt)
> >
> > [https://codeberg.org/ethical-foss/open-slopware#bsd-based](https://codeberg.org/ethical-foss/open-slopware#bsd-based)

> <a id="c_rdfjpr"></a>**oceanhaiyang** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_rdfjpr)
>
> I get your point but I think the Linux kernel being written with LLMs is different than finding every part of the computer that used LLMs, since it is the core of Linux.

> > <a id="c_gvtt4d"></a>**pkubaj** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_gvtt4d)
> >
> > I've found a lot of issues in code using Claude and it greatly helped me write fixes (meaning: a small amount of code, but required a lot of debugging which was the actually hard part that Claude did). I'm not sure about using it for writing some actual new code (features) instead of just reviews. But if it's just auditing and fixing, why not keep improving the code every possible way?
> >
> > Also, if you're after just having the kernel not written with LLM help, then NetBSD is available.

> > > <a id="c_qefv16"></a>**avh-on1** · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_qefv16)
> > >
> > > > why not keep improving the code every possible way?
> > >
> > > Because of the externalities.

> > > <a id="c_5gidaf"></a>**jaypatelani** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_5gidaf)
> > >
> > > Not just NetBSD but haiku and Ironclad OS

> > <a id="c_zh0i4r"></a>**k749gtnc9l3w** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_zh0i4r)
> >
> > C compilers are a worse risk, because they are no less central, but at the same time there is nobody yelling «we do not break the userspace» at breaking changes.

<a id="c_fctue5"></a>**bityard** · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_fctue5)

For a long time, Linus refused to merge any patches having to do with hooking up a debugger to the kernel because he viewed debuggers as a crutch. His reasoning was, a competent programmer should be able to trace every line of code by hand and understand exactly what it does along with all side effects it has. And that finding bugs should be hard because it teaches you not to makes them in the first place.

I guess a leopard can change its shorts.

> <a id="c_h1fcfg"></a>**intelfx** · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_h1fcfg)
>
> That stance you've just cited sounds... quite immature. I'm glad that people do, indeed, change for the better :-)

<a id="c_roftxg"></a>**nishnash** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_roftxg)

He should use something better, Claude is awful for long horizon tasks. People who do agentic coding use a different set of tools than just chat, but IME no amount of tooling could fix those issues with Claude models. Perhaps Opus 5.5 fixes those issues?

<a id="c_oixcbb"></a>**eliasebner** · [13 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_oixcbb)

This saddens me a bit, but oh well, what can I do.

<a id="c_l1kgg5"></a>**giann** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_l1kgg5)

At least he'll be allowed to be as rude as he wants to it

> <a id="c_rr3kf9"></a>**hoistbypetard** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_rr3kf9)
>
> Maybe not... what if this is the story behind [anthropic's recent ban of abusive or cruel behavior toward their LLMs](https://www.theguardian.com/technology/2026/oct/08/anthropic-bans-abusive-behavior-claude)?

> <a id="c_v4ioeo"></a>**k749gtnc9l3w** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_v4ioeo)
>
> Is «fucking bullshit» or «severe breach of trust» more efficient as disapproval when trying to influence LLM output?

<a id="c_8hgeea"></a>**elliotmorris** · [12 October 2026](https://lobste.rs/s/iq5xtw/linus_torvalds_working_on_hefty_linux#c_8hgeea)

\[Comment removed by author\]
