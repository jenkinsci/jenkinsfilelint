node('linux') {
    stage('Deploy') {
        withCredentials([string(credentialsId: 'token', variable: 'TOKEN')]) {
            sh "deploy --token ${TOKEN}"
        }
    }
}
