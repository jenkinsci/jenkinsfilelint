pipeline {
    agent any
    stages {
        stage('S') {
            steps {
                withCredentials([string(credentialsId: 'token', variable: 'TOKEN')]) {
                    sh """
                        curl \\
                          -H "Authorization: Bearer ${TOKEN}" \\
                          https://api.example.com
                    """.stripIndent()
                }
            }
        }
    }
}
