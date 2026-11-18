pipeline {
    agent any
    stages {
        stage('A') {
            steps {
                script {
                    def defaults = [color: 'good']
                    notify(*:defaults)
                    notify(*:defaults, channel: '#ci')
                }
            }
        }
    }
}
