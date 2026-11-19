def call(String id) {
    withCredentials([string(credentialsId: id, variable: 'TOKEN')]) {
        sh "deploy --token ${TOKEN}"
    }
}
