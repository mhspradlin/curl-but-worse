# Curl, but Worse

This is a project to brush up on Rust and write some async Rust.

The general idea is to create a command line utility that, on running, starts
an interactive session that accepts commands and runs multiple asynchronous
activities that then output data back to the console. What's a simple async
activity? Making a GET request.

The commands in the console will be of the format `<url><space><url>...` and
enter will submit the command. The system will then run GETs for the URLs
in parallel (possibly limiting max parllel requests in flight.) It'll then
return the result of the GETs to the console. Another command can be
submitted before all the GETs are done returning.

The user can type `q` or `quit` to close the session.

Concerns:
* Writing to the console should happen in discrete chunks. As in, all of the
  output of one GET should be written before trying to write another.
* GETs can't block each other, otherwise it won't work in parallel!
* Shutdown should be clean, so all in-flight requests finish before closing.
* The session must be able to receive input while it's working on the previous
  command, so processing one command can't block the input of another.

The approach I'll take is to spawn two long-lived tasks, one to manage reading/
writing with the session and another to dispatch and collect results from
commands. They will be connected by two channels, one which sends requests to
load URLs and another that returns the results.

```
+-----------+                        +--------------------+
| Session   | -- Command Channel --> | Request Dispatcher |
|           | <-- Result Channel --  |                    |
+-----------+                        +--------------------+
```

The command channel will have messages of the following format, which are 1:1
with commands that are submitted by the user:
```yaml
commandId: <string> # unique ID for the command
urls: <array of string> # URLs to GET
```

The result channel will have messages of the following format. There will be
one result message for each URL that is requested:
```yaml
commandId: <string> # which command the result is associated with
url: <string> # which URL the result is associated with
output: <string> # the output of the GET request
```

I'm not 100% sure how this will work with Rust's ownership, but I don't think
that there's any reason why we need to track the status of the entire command
(maybe if we wanted to report that the request is all done?) In that case,
the dispatcher will create a task for each GET and pass in a reference to the
input end of the result channel. Each task can then submit its results
directly to the result channel.

To run the tasks I'll use [Tokio](https://tokio.rs/) as an async runtime. It
has [Hyper](https://hyper.rs/) for making the HTTP requests. It doesn't
have anything for writing to the terminal directly. I guess
[Cursive](https://lib.rs/crates/cursive) might be an easy library to use,
by splitting the window into two panes, one with results being displayed
and one that accepts a command? Maybe I'll start with a basic println/
readline and then go to pretty-printing later.

## How did it go?

There was one big change I had to make to the original approach, which was
that for interacting with the terminal I couldn't use async fns but instead
was recommended to spawn a thread to manage the interaction.