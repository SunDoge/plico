namespace Plico;

/// <summary>Injects call-site and component identity into direct component calls.
/// The SDK package supplies the generator. Private components require a partial owner.
/// Use an explicit lambda instead of converting the component to a method-group delegate.</summary>
[AttributeUsage(AttributeTargets.Method, Inherited = false)]
public sealed class ComponentAttribute : Attribute { }
