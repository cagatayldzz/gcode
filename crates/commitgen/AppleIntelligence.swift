import Darwin
import Foundation
#if canImport(FoundationModels)
import FoundationModels
#endif

private struct Request: Decodable {
  let instructions: String
  let prompt: String
}

@main
struct AppleIntelligence {
#if canImport(FoundationModels)
  static func main() async {
    guard #available(macOS 26.0, *) else {
      fail("Apple Intelligence commit generation requires macOS 26 or later.")
    }

    let model = SystemLanguageModel.default
    switch model.availability {
    case .available:
      break
    case .unavailable(.appleIntelligenceNotEnabled):
      fail("Turn on Apple Intelligence in System Settings to generate commit messages on this Mac.")
    case .unavailable(.modelNotReady):
      fail("The Apple Intelligence model is preparing. Try again when it is ready in System Settings.")
    case .unavailable(.deviceNotEligible):
      fail("Apple Intelligence is unavailable on this Mac. Choose another provider in the commit message settings.")
    case .unavailable:
      fail("Apple Intelligence is unavailable. Check System Settings or choose another provider.")
    }

    do {
      let request = try JSONDecoder().decode(Request.self, from: FileHandle.standardInput.readDataToEndOfFile())
      let session = LanguageModelSession(model: model, instructions: request.instructions)
      let options = GenerationOptions(maximumResponseTokens: 192)
      let response = try await session.respond(to: request.prompt, options: options)
      print(response.content)
    } catch LanguageModelSession.GenerationError.exceededContextWindowSize {
      fail("The change exceeds the Apple Intelligence context limit. Select fewer changed files or shorten your commit instructions.")
    } catch LanguageModelSession.GenerationError.unsupportedLanguageOrLocale {
      fail("Apple Intelligence could not process this language. Try another language in the commit message settings.")
    } catch {
      fail("Apple Intelligence could not generate a commit message. Try again or choose another provider.")
    }
  }

#else
  // Built with an SDK older than macOS 26: the model is not there, and the app says so.
  static func main() async {
    fail("This build was made without Apple Intelligence. Choose another provider in the commit message settings.")
  }
#endif

  private static func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data((message + "\n").utf8))
    exit(1)
  }
}
